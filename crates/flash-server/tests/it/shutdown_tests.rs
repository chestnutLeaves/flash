//! Stopping (a deploy's SIGTERM): the request already open finishes,
//! then nothing new is taken, and background workers are told to wind
//! down (`state.shutdown`).

use crate::common;

use std::sync::Arc;

use axum::body::Body;
use axum::http::Request;
use axum::routing::get;
use common::*;
use flash_server::boot;
use tokio::sync::Notify;

#[tokio::test]
async fn a_stop_lets_the_open_request_finish_then_takes_nothing_new() {
    let t = AppBuilder::new("shutdown").build();
    let (running, release) = (Arc::new(Notify::new()), Arc::new(Notify::new()));
    let app = axum::Router::new().route(
        "/slow",
        get({
            let (running, release) = (running.clone(), release.clone());
            || async move {
                running.notify_one();
                release.notified().await;
                "done"
            }
        }),
    );
    let (listener, connector) = memory_listener();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(boot::serve(
        listener,
        app,
        t.state.clone(),
        Vec::new(),
        async move {
            let _ = stopped.await;
        },
    ));

    let open = tokio::spawn({
        let connector = connector.clone();
        async move {
            connector
                .send(Request::get("/slow").body(Body::empty()).unwrap())
                .await
        }
    });
    running.notified().await;
    stop.send(()).unwrap();
    t.state.shutdown.cancelled().await;
    connector.listener_gone().await;
    assert!(!open.is_finished() && !server.is_finished());
    assert!(
        connector
            .send(Request::get("/slow").body(Body::empty()).unwrap())
            .await
            .is_none(),
        "a new request was taken after the stop"
    );

    release.notify_one();
    assert_eq!(
        open.await.unwrap().expect("the open request").text(),
        "done"
    );
    server.await.unwrap();
    // The next boot knows the stop was ours.
    assert!(t.store.server_state(boot::CLEAN_STOP).unwrap().is_some());
}
