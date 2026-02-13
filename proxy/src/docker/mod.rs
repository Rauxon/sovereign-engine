pub mod llamacpp;

use std::collections::HashMap;
use std::collections::HashSet;

use anyhow::{Context, Result};
use bollard::models::NetworkCreateRequest;
use bollard::query_parameters::{CreateImageOptions, ListContainersOptions};
use bollard::Docker;
use futures::StreamExt;
use rand::RngExt;
use tracing::{info, warn};

use serde::Serialize;

use crate::config::AppConfig;

/// Full GPU statistics including memory and compute utilization.
#[derive(Debug, Clone, Serialize)]
pub struct GpuStats {
    pub gpu_type: String,
    pub device_index: u32,
    pub total_mb: u64,
    pub used_mb: u64,
    pub free_mb: u64,
    /// GPU compute utilization 0–100, if available.
    pub utilization_percent: Option<u64>,
}

const LABEL_MANAGED_BY: &str = "managed-by";
const LABEL_MANAGED_VALUE: &str = "sovereign-engine";
const LABEL_MODEL_ID: &str = "sovereign-engine.model-id";
pub(crate) const LABEL_BACKEND: &str = "sovereign-engine.backend";

#[derive(Debug, Clone)]
pub struct DockerManager {
    pub docker: Docker,
    pub model_path: String,
    pub backend_network: String,
}

impl DockerManager {
    /// Create a dummy DockerManager for tests (no real Docker connection needed).
    #[cfg(test)]
    pub(crate) fn test_dummy() -> Self {
        let docker =
            Docker::connect_with_http("http://localhost:1", 1, bollard::API_DEFAULT_VERSION)
                .expect("dummy Docker client");
        Self {
            docker,
            model_path: "/tmp/test-models".to_string(),
            backend_network: "test-network".to_string(),
        }
    }

    pub async fn new(config: &AppConfig) -> Result<Self> {
        let docker =
            Docker::connect_with_local_defaults().context("Failed to connect to Docker")?;

        // Verify Docker connectivity
        let version = docker
            .version()
            .await
            .context("Failed to get Docker version — is the Docker socket mounted?")?;

        info!(
            docker_version = version.version.as_deref().unwrap_or("unknown"),
            "Connected to Docker"
        );

        // Ensure the internal backend network exists
        let network_name = &config.backend_network;
        match docker.inspect_network(network_name, None).await {
            Ok(_) => {
                info!(network = %network_name, "Backend network exists");
            }
            Err(_) => {
                info!(network = %network_name, "Creating backend internal network");
                docker
                    .create_network(NetworkCreateRequest {
                        name: network_name.clone(),
                        driver: Some("bridge".to_string()),
                        internal: Some(true),
                        ..Default::default()
                    })
                    .await
                    .context("Failed to create backend internal network")?;
            }
        }

        Ok(Self {
            docker,
            model_path: config.model_host_path.clone(),
            backend_network: config.backend_network.clone(),
        })
    }

    /// List all containers managed by Sovereign Engine.
    pub async fn list_managed_containers(&self) -> Result<Vec<bollard::models::ContainerSummary>> {
        let mut filters = HashMap::new();
        filters.insert(
            "label".to_string(),
            vec![format!("{}={}", LABEL_MANAGED_BY, LABEL_MANAGED_VALUE)],
        );

        let containers = self
            .docker
            .list_containers(Some(ListContainersOptions {
                all: true,
                filters: Some(filters),
                ..Default::default()
            }))
            .await
            .context("Failed to list containers")?;

        Ok(containers)
    }

    /// Allocate a random UID in 10000–65000, avoiding collisions with running containers.
    pub async fn allocate_uid(&self) -> Result<u32> {
        const UID_MIN: u32 = 10000;
        const UID_MAX: u32 = 65000;

        // Collect UIDs already in use by managed containers
        let containers = self.list_managed_containers().await?;
        let mut used_uids = HashSet::new();
        for c in &containers {
            if let Some(id) = &c.id {
                if let Ok(detail) = self.docker.inspect_container(id, None).await {
                    if let Some(config) = &detail.config {
                        if let Some(user) = &config.user {
                            // user field is "uid:gid" or just "uid"
                            if let Some(uid_str) = user.split(':').next() {
                                if let Ok(uid) = uid_str.parse::<u32>() {
                                    used_uids.insert(uid);
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut rng = rand::rng();
        for _ in 0..100 {
            let candidate = rng.random_range(UID_MIN..=UID_MAX);
            if !used_uids.contains(&candidate) {
                return Ok(candidate);
            }
        }

        anyhow::bail!("Failed to allocate unique UID after 100 attempts")
    }

    /// Get the internal base URL for a backend container on the isolated network.
    pub fn backend_base_url(&self, model_id: &str, backend_type: &str) -> String {
        match backend_type {
            "llamacpp" => self.llamacpp_base_url(model_id),
            other => panic!("Unknown backend type: {other}"),
        }
    }

    /// Stop a backend container by model ID.
    pub async fn stop_backend(&self, model_id: &str, backend_type: &str) -> Result<()> {
        match backend_type {
            "llamacpp" => self.stop_llamacpp(model_id).await,
            other => anyhow::bail!("Unknown backend type: {other}"),
        }
    }

    /// Check if a backend container is healthy and responding.
    pub async fn check_backend_health(&self, model_id: &str, backend_type: &str) -> Result<bool> {
        match backend_type {
            "llamacpp" => self.check_llamacpp_health(model_id).await,
            other => anyhow::bail!("Unknown backend type: {other}"),
        }
    }

    /// Detect available GPU types by checking Docker runtime capabilities and device nodes.
    pub async fn detect_gpu(&self) -> Vec<String> {
        let mut gpus = Vec::new();

        // Vulkan works on any GPU with DRI support (AMD and NVIDIA)
        if std::path::Path::new("/dev/dri").exists() {
            gpus.push("vulkan".to_string());
        }

        gpus
    }

    /// Pull backend container images based on detected GPU capabilities.
    ///
    /// Pulls happen concurrently in the background. Each image is pulled only if
    /// not already present locally.
    pub async fn pull_backend_images(&self) {
        let gpus = self.detect_gpu().await;

        // Always pull llama.cpp CPU
        let mut images = vec![llamacpp::LLAMACPP_IMAGE_CPU];

        if gpus.contains(&"vulkan".to_string()) {
            images.push(llamacpp::LLAMACPP_IMAGE_VULKAN);
        }

        info!(images = ?images, "Pulling backend images in background");

        for image in images {
            let docker = self.docker.clone();
            tokio::spawn(async move {
                pull_image(&docker, image).await;
            });
        }
    }

    /// Determine which backends are available based on detected GPUs.
    pub async fn available_backends(&self) -> Vec<String> {
        vec!["llamacpp".to_string()]
    }

    /// Collect GPU stats from all detected GPUs (NVIDIA + AMD).
    /// Returns empty vec if no GPUs are detected.
    pub async fn gpu_all_info() -> Vec<GpuStats> {
        let mut all = Vec::new();
        all.extend(Self::gpu_stats_nvidia().await);
        all.extend(Self::gpu_stats_amdgpu_sysfs());
        all
    }

    /// Query all NVIDIA GPUs via nvidia-smi. Returns one GpuStats per GPU.
    async fn gpu_stats_nvidia() -> Vec<GpuStats> {
        let output = match tokio::process::Command::new("nvidia-smi")
            .args([
                "--query-gpu=memory.total,memory.used,memory.free,utilization.gpu",
                "--format=csv,noheader,nounits",
            ])
            .output()
            .await
        {
            Ok(o) if o.status.success() => o,
            _ => return vec![],
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout
            .lines()
            .enumerate()
            .filter_map(|(idx, line)| {
                let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
                if parts.len() < 3 {
                    return None;
                }
                let total: u64 = parts[0].parse().ok()?;
                let used: u64 = parts[1].parse().ok()?;
                let free: u64 = parts[2].parse().ok()?;
                let utilization_percent = parts.get(3).and_then(|s| s.parse::<u64>().ok());
                Some(GpuStats {
                    gpu_type: "nvidia".to_string(),
                    device_index: idx as u32,
                    total_mb: total,
                    used_mb: used,
                    free_mb: free,
                    utilization_percent,
                })
            })
            .collect()
    }

    /// Enumerate AMD GPUs via kernel sysfs interface. No CLI tools needed.
    /// Reads /sys/class/drm/card*/device/mem_info_vram_total etc.
    ///
    /// On unified memory platforms (e.g. AMD Strix Halo APUs), the GPU can
    /// also access system RAM via GTT. We report VRAM + GTT as the total so
    /// the dashboard reflects actually usable memory. This overstates capacity
    /// on discrete GPUs where GTT is a separate, slower pool — but currently
    /// the target hardware is Strix Halo.
    fn gpu_stats_amdgpu_sysfs() -> Vec<GpuStats> {
        let mut results = Vec::new();
        let drm_dir = match std::fs::read_dir("/sys/class/drm") {
            Ok(d) => d,
            Err(_) => return results,
        };

        let mut card_indices: Vec<u32> = Vec::new();
        for entry in drm_dir.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            // Match "card0", "card1", etc — skip "card0-DP-1" style entries
            if let Some(rest) = name_str.strip_prefix("card") {
                if let Ok(idx) = rest.parse::<u32>() {
                    card_indices.push(idx);
                }
            }
        }
        card_indices.sort();

        for (device_index, card_idx) in card_indices.iter().enumerate() {
            let device_path = format!("/sys/class/drm/card{}/device", card_idx);
            let vram_total_path = format!("{}/mem_info_vram_total", device_path);
            let vram_used_path = format!("{}/mem_info_vram_used", device_path);
            let gtt_total_path = format!("{}/mem_info_gtt_total", device_path);
            let gtt_used_path = format!("{}/mem_info_gtt_used", device_path);
            let busy_path = format!("{}/gpu_busy_percent", device_path);

            let vram_total_bytes = match read_sysfs_u64(&vram_total_path) {
                Some(v) => v,
                None => continue, // Not an amdgpu card
            };
            let vram_used_bytes = read_sysfs_u64(&vram_used_path).unwrap_or(0);
            let gtt_total_bytes = read_sysfs_u64(&gtt_total_path).unwrap_or(0);
            let gtt_used_bytes = read_sysfs_u64(&gtt_used_path).unwrap_or(0);

            let total_bytes = vram_total_bytes + gtt_total_bytes;
            let used_bytes = vram_used_bytes + gtt_used_bytes;
            let total_mb = total_bytes / (1024 * 1024);
            let used_mb = used_bytes / (1024 * 1024);
            let free_mb = total_mb.saturating_sub(used_mb);
            let utilization_percent = read_sysfs_u64(&busy_path);

            results.push(GpuStats {
                gpu_type: "amdgpu".to_string(),
                device_index: device_index as u32,
                total_mb,
                used_mb,
                free_mb,
                utilization_percent,
            });
        }

        results
    }

    /// Get host PIDs for a managed container by name via Docker top.
    async fn get_container_pids(&self, container_name: &str) -> Vec<u32> {
        let top = match self.docker.top_processes(container_name, None).await {
            Ok(t) => t,
            Err(_) => return vec![],
        };

        let processes = match top.processes {
            Some(p) => p,
            None => return vec![],
        };

        // Find the PID column index from titles (usually "PID")
        let titles = top.titles.unwrap_or_default();
        let pid_col = titles.iter().position(|t| t == "PID").unwrap_or(1); // fallback to column 1

        processes
            .iter()
            .filter_map(|row| row.get(pid_col)?.parse::<u32>().ok())
            .collect()
    }

    /// Per-container VRAM usage: model_id → total VRAM MB.
    /// Uses DRM fdinfo for AMD GPUs and nvidia-smi for NVIDIA GPUs.
    /// Requires `pid: host` in docker-compose for host PID visibility.
    pub async fn per_container_vram(&self) -> HashMap<String, u64> {
        let containers = match self.list_managed_containers().await {
            Ok(c) => c,
            Err(_) => return HashMap::new(),
        };

        // Build model_id → host PIDs mapping
        let mut model_pids: Vec<(String, Vec<u32>)> = Vec::new();
        for c in &containers {
            let labels = c.labels.as_ref();
            let model_id = match labels.and_then(|l| l.get(LABEL_MODEL_ID)) {
                Some(id) => id.clone(),
                None => continue,
            };
            // Only check running containers
            if c.state != Some(bollard::models::ContainerSummaryStateEnum::RUNNING) {
                continue;
            }
            let name = c
                .names
                .as_ref()
                .and_then(|n| n.first())
                .map(|n| n.trim_start_matches('/').to_string())
                .unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            let pids = self.get_container_pids(&name).await;
            if !pids.is_empty() {
                model_pids.push((model_id, pids));
            }
        }

        if model_pids.is_empty() {
            return HashMap::new();
        }

        // Collect all PIDs into a set for nvidia-smi lookup
        let all_pids: HashSet<u32> = model_pids
            .iter()
            .flat_map(|(_, pids)| pids.iter().copied())
            .collect();

        // NVIDIA per-process VRAM (best-effort)
        let nvidia_pid_vram = Self::nvidia_per_process_vram().await;

        // DRM fdinfo per-process VRAM for AMD (best-effort)
        let amd_pid_vram = Self::drm_fdinfo_vram(&all_pids);

        // Merge: for each model, sum VRAM from all its PIDs across both sources
        let mut result = HashMap::new();
        for (model_id, pids) in &model_pids {
            let mut total_vram_bytes: u64 = 0;
            for &pid in pids {
                if let Some(&mb) = nvidia_pid_vram.get(&pid) {
                    total_vram_bytes += mb * 1024 * 1024; // nvidia reports in MiB
                }
                if let Some(&bytes) = amd_pid_vram.get(&pid) {
                    total_vram_bytes += bytes;
                }
            }
            if total_vram_bytes > 0 {
                result.insert(model_id.clone(), total_vram_bytes / (1024 * 1024));
            }
        }

        result
    }

    /// Query NVIDIA per-process GPU memory via nvidia-smi.
    /// Returns PID → used memory in MiB.
    async fn nvidia_per_process_vram() -> HashMap<u32, u64> {
        let output = match tokio::process::Command::new("nvidia-smi")
            .args([
                "--query-compute-apps=pid,used_memory",
                "--format=csv,noheader,nounits",
            ])
            .output()
            .await
        {
            Ok(o) if o.status.success() => o,
            _ => return HashMap::new(),
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut map = HashMap::new();
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
            if parts.len() >= 2 {
                if let (Ok(pid), Ok(mem)) = (parts[0].parse::<u32>(), parts[1].parse::<u64>()) {
                    *map.entry(pid).or_insert(0) += mem;
                }
            }
        }
        map
    }

    /// Read DRM fdinfo for a set of PIDs to extract AMD VRAM usage.
    /// Returns PID → VRAM bytes. Deduplicates by drm-client-id.
    fn drm_fdinfo_vram(pids: &HashSet<u32>) -> HashMap<u32, u64> {
        let mut result = HashMap::new();

        for &pid in pids {
            let fdinfo_dir = format!("/proc/{}/fdinfo", pid);
            let entries = match std::fs::read_dir(&fdinfo_dir) {
                Ok(e) => e,
                Err(_) => continue,
            };

            let mut seen_clients: HashSet<String> = HashSet::new();
            let mut pid_vram: u64 = 0;

            for entry in entries.flatten() {
                let content = match std::fs::read_to_string(entry.path()) {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                // Only process DRM fdinfo entries
                if !content.contains("drm-memory-vram") {
                    continue;
                }

                // Extract drm-client-id for dedup
                let client_id = content
                    .lines()
                    .find(|l| l.starts_with("drm-client-id:"))
                    .map(|l| l.trim().to_string());

                if let Some(ref cid) = client_id {
                    if !seen_clients.insert(cid.clone()) {
                        continue; // Already counted this client
                    }
                }

                // Parse "drm-memory-vram:\t1234 KiB" or similar
                for line in content.lines() {
                    if let Some(rest) = line.strip_prefix("drm-memory-vram:") {
                        let rest = rest.trim();
                        // Format: "<value> <unit>" e.g. "1234 KiB"
                        let parts: Vec<&str> = rest.split_whitespace().collect();
                        if let Some(Ok(val)) = parts.first().map(|s| s.parse::<u64>()) {
                            let unit = parts.get(1).copied().unwrap_or("KiB");
                            let bytes = match unit {
                                "B" => val,
                                "KiB" => val * 1024,
                                "MiB" => val * 1024 * 1024,
                                "GiB" => val * 1024 * 1024 * 1024,
                                _ => val * 1024, // default to KiB
                            };
                            pid_vram += bytes;
                        }
                    }
                }
            }

            if pid_vram > 0 {
                result.insert(pid, pid_vram);
            }
        }

        result
    }
}

/// Read a u64 from a sysfs file (trimmed). Returns None if file doesn't exist or parse fails.
fn read_sysfs_u64(path: &str) -> Option<u64> {
    std::fs::read_to_string(path)
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
}

/// Pull a single Docker image, logging progress. No-op if already present.
async fn pull_image(docker: &Docker, image: &str) {
    // Split image into repo and tag
    let (repo, tag) = match image.rsplit_once(':') {
        Some((r, t)) => (r, t),
        None => (image, "latest"),
    };

    // Check if image already exists locally
    let full_ref = format!("{}:{}", repo, tag);
    if docker.inspect_image(&full_ref).await.is_ok() {
        info!(image = %full_ref, "Image already present");
        return;
    }

    info!(image = %full_ref, "Pulling image...");

    let mut stream = docker.create_image(
        Some(CreateImageOptions {
            from_image: Some(repo.to_string()),
            tag: Some(tag.to_string()),
            ..Default::default()
        }),
        None,
        None,
    );

    while let Some(result) = stream.next().await {
        match result {
            Ok(_) => {} // Progress chunk — just consume it
            Err(e) => {
                warn!(image = %full_ref, error = %e, "Failed to pull image");
                return;
            }
        }
    }

    info!(image = %full_ref, "Image pulled successfully");
}
