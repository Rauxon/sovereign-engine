-- Migrate any existing vLLM models to llama.cpp backend
UPDATE models SET backend_type = 'llamacpp' WHERE backend_type = 'vllm';
