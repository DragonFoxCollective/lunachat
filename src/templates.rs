use askama::Template;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use serde::{Deserialize, Serialize};

#[derive(Template)]
#[template(path = "index.html.jinja")]
pub struct IndexTemplate;

#[derive(Template)]
#[template(path = "forum.html.jinja")]
pub struct ForumTemplate {
    pub username: Option<String>,
    pub posts: String,
    pub can_post: bool,
}

#[derive(Template, Clone, Serialize, Deserialize)]
#[template(path = "post.html.jinja")]
pub struct PostTemplate {
    pub author: String,
    pub avatar: Option<String>,
    pub body: String,
}

#[derive(Template)]
#[template(path = "login.html.jinja")]
pub struct LoginTemplate {
    pub error: Option<String>,
    pub next: Option<String>,
}

/// A wrapper type that we'll use to encapsulate HTML parsed by askama into valid HTML for axum to serve.
pub struct HtmlTemplate<T>(pub T);

/// Allows us to convert Askama HTML templates into valid HTML for axum to serve in the response.
impl<T> IntoResponse for HtmlTemplate<T>
where
    T: Template,
{
    fn into_response(self) -> Response {
        // Attempt to render the template with askama
        match self.0.render() {
            // If we're able to successfully parse and aggregate the template, serve it
            Ok(html) => Html(html).into_response(),
            // If we're not, return an error or some bit of fallback HTML
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to render template. Error: {}", err),
            )
                .into_response(),
        }
    }
}
