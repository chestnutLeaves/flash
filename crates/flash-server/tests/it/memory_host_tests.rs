//! `MemoryHost::router` takes a request's body whole, read or not.

use std::time::Duration;

use axum::routing::post;
use axum::Router;
use flash_server::testing::{MemoryHost, PIPE_BYTES};

#[test]
fn a_fake_that_ignores_the_body_still_takes_it_whole() {
    let host = MemoryHost::router(Router::new().route("/v1/things", post(|| async { "made" })));
    let agent = host.agent(Duration::from_secs(300), false);
    let body = vec![b'x'; 4 * PIPE_BYTES];
    let reply = agent
        .post("http://fake.test/v1/things")
        .send(&body[..])
        .unwrap();
    assert_eq!(reply.into_body().read_to_string().unwrap(), "made");
}
