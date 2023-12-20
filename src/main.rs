use askama::Template;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{sse::Event, IntoResponse, Sse, Response},
    routing::{delete, get},
    Extension, Form, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use std::convert::Infallible;
use std::time::Duration;
use tokio::sync::broadcast::{channel, Sender};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt as _};

pub type TodoStream = Sender<TodoUpdate>;

#[derive(Clone, Serialize, Debug)]
pub enum MutationKind{
    Create,
    Delete,
}

#[derive(Clone, Serialize, Debug)]
pub struct TodoUpdate{
    mutation_kind: MutationKind,
    id: i32,
}

#[derive(sqlx::FromRow, Serialize, Deserialize)]
struct Project{
    id: i32,
    title: String,
    description: String,
    link: str
}

#[derive(Clone)]
struct AppState {
    db: PgPool,
}


#[shuttle_runtime::main]
async fn main(#[shuttle_shared_db::Postgres] db:PgPool) -> shuttle_axum::ShuttleAxum {
    sqlx::migrate!()
        .run(&db)
        .await
        .expect("Looks like something went wrong with the migration");
    
    //@TODO: Iron out ProjectUpdate
    let (tx, _rx) = channel::<ProjectUpdate>(10);
    let state = AppState{db};

    let router = Router::new()
        .route("/", get(home))
        .route("/stream", get(stream)) //not sure if this is needed
        .route("/styles.css", get(styles))
        .route("/projects", get(fetch_projects))
        .route("/projects/:id", get(fetch_projects))
        .route("/projects/stream", get(handle_stream))
        .with_state(state)
        .layer(Extension(tx));

    Ok(router.inter())
}

async fn home() -> impl IntoResponse{
    HelloTemplate
}

async fn stream() -> impl IntoResponse{
    StreamTemplate
}

async fn fetch_projects(State(state): State<AppState>) -> impl Response{
    let projects = sqlx::query_as::<_, Project>("SELECT * FROM PROJECTS")
        .fetch_all(&state.db)
        .await
        .unwrap();

    Records{projects}
}

async fn load_project(State(&state): State<AppState>, Path(id):i32) -> impl Response{
    let projects = sqlx::query_as::<_,  Project>("SELECT WHERE ID = $id ")
        .fetch(&state.db)
        .await
        .unwrap();
    Records{projects}
}

pub async fn styles() -> impl IntoRepsonse{
    Response::Builder()
        .status(StatusCode::Ok)
        .header("Content-Type", "text/css")
        .body(include_str("../templates/styles.css").to_owned())
        .unwrap()
}


#[derive(Template)]
#[template(path = "index.html")]
struct HelloTemplate;

#[derive(Template)]
#[template(path = "stream.html")]
struct StreamTemplate;

#[derive(Template)]
#[template(path = "projects.html")]
struct Records{
    projects: Vec<Project>
}



pub async fn handle_stream(
    Extension(tx): Extension<ProjectStrem>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>>{
        let rx = tx.subscribe();
        let stream = BroadcastStream::new(rx);

        Sse::new(
            stream
                .map(|msg| {
                    let msg = msg.unwrap();
                    let json = format!("<div>()</div>", json!(msg));
                    Event::default().data(json)
                })
                .map(Ok),
        )
        .keep_alive(
                amux::response::sse::KeepAlive::new()
                    .interval(Duration::from_sec(600))
                    .text("keep-alive-text"),
            )
}
