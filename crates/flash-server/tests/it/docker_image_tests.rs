//! What the Docker image must keep across releases.
//!
//! Files in a volume keep their numeric owner. The 0.1.0 image gave its
//! `flash` user uid 100 and gid 101, so every database and media file a
//! self-hoster has in `/data` is owned by those ids, and a later image
//! whose user has any other ids cannot open them: the server boots into
//! "attempt to write a readonly database". The Dockerfile pins the ids;
//! this test keeps the pin.

use std::path::Path;

fn dockerfile() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Dockerfile");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn the_flash_user_keeps_the_ids_the_first_image_gave_it() {
    let text = dockerfile();
    let one_line: String = text
        .lines()
        .map(|l| l.trim_end_matches('\\').trim())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        one_line.contains("groupadd --system --gid 101 flash"),
        "the flash group must be created with gid 101"
    );
    assert!(
        one_line.contains("useradd --system --uid 100 --gid flash"),
        "the flash user must be created with uid 100"
    );
    assert!(text.contains("USER flash"), "the image must run as flash");
    assert!(
        text.contains("VOLUME /data"),
        "the data directory must be a volume"
    );
}
