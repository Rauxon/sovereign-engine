pub mod admin;
pub mod error;
pub mod hf;
pub mod openai;
pub mod reservation;
pub mod user;

use std::sync::Arc;

use axum::middleware;
use axum::Router;

use crate::auth::admin_only_middleware;
use crate::AppState;

pub fn routes(state: Arc<AppState>) -> Router {
    let admin_routes = admin::routes(state.clone())
        .merge(reservation::admin_routes(state.clone()))
        .layer(middleware::from_fn(admin_only_middleware));

    Router::new()
        .nest("/admin", admin_routes)
        .nest("/user", user::routes(state.clone()))
        .nest("/user", reservation::user_routes(state.clone()))
        .nest("/user/hf", hf::routes(state))
}
