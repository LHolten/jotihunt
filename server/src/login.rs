use axum::{
    extract::Form,
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;

use crate::AppState;

#[derive(Deserialize)]
pub struct LoginForm {
    password: String,
    /// The client page to go back to.
    redirect: String,
}

/// Sends the browser back to the client with the result in the fragment,
/// which is never sent to a server. The client removes it from the url again.
pub async fn login(
    state: &'static AppState,
    password: &str,
    Form(form): Form<LoginForm>,
) -> Response {
    if form.password != password {
        return Redirect::to(&format!("{}#wrong", form.redirect)).into_response();
    }
    Redirect::to(&format!("{}#secret={}", form.redirect, state.secret)).into_response()
}

#[tokio::test]
async fn login_redirects() {
    let state = crate::leak(AppState::new(
        rust_query::migration::Config::open_in_memory(),
    ));
    let location = |password: &str, redirect: &str| {
        let form = LoginForm {
            password: password.to_owned(),
            redirect: redirect.to_owned(),
        };
        async move {
            let res = login(state, "test", Form(form)).await;
            let location = res.headers().get("location");
            location.map(|l| l.to_str().unwrap().to_owned())
        }
    };
    let client = "https://lholten.github.io/jotihunt/";
    let secret = format!("{client}#secret={}", state.secret);
    assert_eq!(location("test", client).await, Some(secret));
    assert_eq!(
        location("nope", client).await,
        Some(format!("{client}#wrong"))
    );
}
