//! What the composer may take, as one JSON value both the generate page
//! and `/meta` publish: the kinds this server reads with the extensions
//! and types a picker should accept, the per-file and per-set limits,
//! the paste rule, and every sentence a client shows for a refusal it
//! can make on its own before uploading. The sentences live here, in
//! Rust, so no client invents wording of its own and the same file is
//! refused in the same words wherever it is dropped.

use serde::Serialize;

use super::sources::{DOCUMENTS, NO_CONVERTER, TEXT_FILES, UNSUPPORTED_UPLOAD};
use crate::bounds;
use crate::policy::AiInput;
use crate::state::AiRuntime;

/// A paste longer than this many characters, or with this many lines,
/// is material rather than an instruction and becomes a source.
pub const LIFT_CHARS: u32 = 1_000;
pub const LIFT_LINES: u32 = 8;

/// One kind the picker offers. `document` is the office files the
/// converter turns into PDFs, listed only when it can.
#[derive(Debug, Clone, Serialize)]
pub struct KindCaps {
    pub kind: &'static str,
    /// Extensions with their dot, lowercase: what `accept=` lists.
    pub ext: Vec<String>,
    pub mime: Vec<&'static str>,
    /// The plan's refusal of this kind for the person the page is
    /// rendered for; null on `/meta`, which knows no person.
    pub withheld: Option<Withheld>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Withheld {
    pub code: &'static str,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SetCaps {
    pub max_sources: usize,
    pub max_images: u32,
    pub max_pages: u32,
    pub max_words: u32,
    pub max_media_seconds: u32,
    pub max_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Lift {
    pub chars: u32,
    pub lines: u32,
}

/// The sentences a client may show without asking the server.
#[derive(Debug, Clone, Serialize)]
pub struct Copy {
    pub unsupported: &'static str,
    pub document: &'static str,
    pub too_large: String,
    pub too_many_sources: String,
    pub too_many_images: String,
    pub folder: &'static str,
    pub name_too_long: &'static str,
    pub empty: &'static str,
    pub busy: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct Caps {
    pub kinds: Vec<KindCaps>,
    pub youtube: bool,
    pub file_max_bytes: u64,
    pub filename_max: usize,
    pub url_max: usize,
    pub text_max_bytes: usize,
    pub instruction_max: usize,
    pub set: SetCaps,
    pub lift: Lift,
    pub copy: Copy,
}

/// The stored kind of a media-module kind.
fn kind_of(media: flash_store::media::MediaKind) -> Option<AiInput> {
    use flash_store::media::MediaKind;
    match media {
        MediaKind::Image => Some(AiInput::Image),
        MediaKind::Audio => Some(AiInput::Audio),
        MediaKind::Video => Some(AiInput::Video),
        MediaKind::Unsupported => None,
    }
}

fn push_unique<T: PartialEq>(list: &mut Vec<T>, item: T) {
    if !list.contains(&item) {
        list.push(item);
    }
}

impl Caps {
    /// What `ai` reads, each kind marked withheld when `admissions` says
    /// the plan refuses it (`ai_kind_admissions` for a person; empty
    /// for `/meta`).
    pub fn of(ai: &AiRuntime, admissions: &[crate::service::KindAdmission]) -> Caps {
        let withheld_of = |kind: AiInput| -> Option<Withheld> {
            admissions
                .iter()
                .find(|(k, _)| *k == kind)
                .and_then(|(_, verdict)| verdict.as_ref())
                .map(|(code, reason)| Withheld {
                    code,
                    reason: reason.clone(),
                })
        };
        let mut kinds = Vec::new();
        if ai.accepts(AiInput::Text) {
            let mut ext = Vec::new();
            let mut mime = Vec::new();
            for (e, m) in TEXT_FILES {
                push_unique(&mut ext, format!(".{e}"));
                push_unique(&mut mime, *m);
            }
            kinds.push(KindCaps {
                kind: "text",
                ext,
                mime,
                withheld: withheld_of(AiInput::Text),
            });
        }
        if ai.accepts(AiInput::Pdf) {
            kinds.push(KindCaps {
                kind: "pdf",
                ext: vec![".pdf".into()],
                mime: vec!["application/pdf"],
                withheld: withheld_of(AiInput::Pdf),
            });
            if ai.converts_documents() {
                let mut ext = Vec::new();
                let mut mime = Vec::new();
                for (e, m) in DOCUMENTS {
                    push_unique(&mut ext, format!(".{e}"));
                    push_unique(&mut mime, *m);
                }
                kinds.push(KindCaps {
                    kind: "document",
                    ext,
                    mime,
                    withheld: withheld_of(AiInput::Pdf),
                });
            }
        }
        for kind in [
            AiInput::Image,
            AiInput::Youtube,
            AiInput::Video,
            AiInput::Audio,
        ] {
            if !ai.accepts(kind) {
                continue;
            }
            let mut ext = Vec::new();
            let mut mime = Vec::new();
            for (e, media_kind, m) in flash_store::media::EXTENSIONS {
                if kind_of(*media_kind) == Some(kind) && ai.accepts_mime(kind, m) {
                    push_unique(&mut ext, format!(".{e}"));
                    push_unique(&mut mime, *m);
                }
            }
            kinds.push(KindCaps {
                kind: kind.as_str(),
                ext,
                mime,
                withheld: withheld_of(kind),
            });
        }
        Caps {
            youtube: ai.accepts(AiInput::Youtube) && ai.capabilities.supports_youtube,
            kinds,
            file_max_bytes: flash_store::media::MAX_FILE_BYTES,
            filename_max: bounds::FILENAME,
            url_max: bounds::YOUTUBE_URL,
            text_max_bytes: bounds::AI_TEXT_SOURCE,
            instruction_max: bounds::AI_INSTRUCTION,
            set: SetCaps {
                max_sources: bounds::AI_SOURCES_PER_JOB,
                max_images: bounds::AI_MAX_IMAGES,
                max_pages: bounds::AI_MAX_PAGES,
                max_words: bounds::AI_MAX_WORDS,
                max_media_seconds: bounds::AI_MAX_MEDIA_SECONDS,
                max_bytes: bounds::AI_MAX_SET_BYTES,
            },
            lift: Lift {
                chars: LIFT_CHARS,
                lines: LIFT_LINES,
            },
            copy: Copy {
                unsupported: UNSUPPORTED_UPLOAD,
                document: NO_CONVERTER,
                too_large: crate::service::upload_too_large(),
                too_many_sources: crate::service::too_many_sources(),
                too_many_images: crate::service::too_many_photos(),
                folder: "That's a folder. Drop the files inside it.",
                name_too_long: "The file's name is too long.",
                empty: "The file is empty.",
                busy: "Waiting for a free upload slot.",
            },
        }
    }

    /// The value of a file input's `accept` attribute: every extension
    /// and type of every kind that carries files.
    pub fn accept_attr(&self) -> String {
        let mut items: Vec<String> = Vec::new();
        for kind in &self.kinds {
            for e in &kind.ext {
                push_unique(&mut items, e.clone());
            }
            for m in &kind.mime {
                push_unique(&mut items, (*m).to_string());
            }
        }
        items.join(",")
    }

    /// One sentence naming what may be dropped, for the composer's hint.
    pub fn hint(&self) -> String {
        let mut names: Vec<&str> = Vec::new();
        for kind in &self.kinds {
            names.push(match kind.kind {
                "text" => "text files",
                "pdf" => "PDFs",
                "document" => "documents",
                "image" => "photos",
                "video" => "video",
                "audio" => "audio",
                _ => continue,
            });
        }
        let what = match names.len() {
            0 => "pasted text".to_string(),
            1 => names[0].to_string(),
            n => format!("{} or {}", names[..n - 1].join(", "), names[n - 1]),
        };
        let mb = self.file_max_bytes / (1024 * 1024);
        if self.youtube {
            format!("Drop {what}, or paste a YouTube link. Up to {mb} MB a file.")
        } else {
            format!("Drop {what}. Up to {mb} MB a file.")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::fake::FakeProvider;
    use crate::ai::provider::Capabilities;
    use std::sync::Arc;

    fn runtime(caps: Option<Capabilities>, inputs: Option<Vec<AiInput>>) -> AiRuntime {
        let mut fake = FakeProvider::default();
        if let Some(caps) = caps {
            fake = fake.with_capabilities(caps);
        }
        AiRuntime::new(
            Arc::new(fake),
            inputs,
            #[cfg(feature = "pdf")]
            None,
            None,
            std::env::temp_dir(),
        )
        .unwrap()
    }

    #[test]
    fn the_picker_lists_only_what_the_model_reads_and_the_operator_allows() {
        let all = runtime(None, Some(AiInput::ALL.to_vec()));
        let caps = Caps::of(&all, &[]);
        let kinds: Vec<&str> = caps.kinds.iter().map(|k| k.kind).collect();
        assert_eq!(kinds, ["text", "pdf", "image", "youtube", "video", "audio"]);
        assert!(caps.youtube);
        let image = caps.kinds.iter().find(|k| k.kind == "image").unwrap();
        assert!(image.ext.contains(&".png".to_string()));
        assert!(image.mime.contains(&"image/png"));
        assert!(caps.accept_attr().contains(".pdf,application/pdf"));
        assert!(caps.accept_attr().contains(".md"));
        assert!(caps.hint().contains("YouTube link"), "{}", caps.hint());
        assert_eq!(caps.copy.unsupported, UNSUPPORTED_UPLOAD);
        assert_eq!(caps.lift.chars, 1_000);
        assert_eq!(caps.set.max_sources, 40);

        // Narrowed by the operator: no links, no video.
        let narrow = runtime(None, Some(vec![AiInput::Text, AiInput::Image]));
        let caps = Caps::of(&narrow, &[]);
        let kinds: Vec<&str> = caps.kinds.iter().map(|k| k.kind).collect();
        assert_eq!(kinds, ["text", "image"]);
        assert!(!caps.youtube);
        assert!(!caps.hint().contains("YouTube"));
        assert_eq!(
            caps.hint(),
            "Drop text files or photos. Up to 100 MB a file."
        );

        // Narrowed by the model: a type the model does not read is not
        // offered even though the editor knows it.
        let picky = runtime(
            Some(Capabilities {
                mimes: vec![(AiInput::Image, vec!["image/png"])],
                ..FakeProvider::everything()
            }),
            Some(vec![AiInput::Image]),
        );
        let image = &Caps::of(&picky, &[]).kinds[0];
        assert_eq!(image.ext, [".png"]);
        assert_eq!(image.mime, ["image/png"]);
    }

    #[test]
    fn a_withheld_kind_carries_the_plans_own_sentence() {
        let all = runtime(None, Some(AiInput::ALL.to_vec()));
        let caps = Caps::of(
            &all,
            &[
                (AiInput::Text, None),
                (
                    AiInput::Video,
                    Some(("pro_required", "Video needs a bigger plan.".to_string())),
                ),
            ],
        );
        let video = caps.kinds.iter().find(|k| k.kind == "video").unwrap();
        let withheld = video.withheld.as_ref().unwrap();
        assert_eq!(withheld.code, "pro_required");
        assert_eq!(withheld.reason, "Video needs a bigger plan.");
        assert!(caps
            .kinds
            .iter()
            .find(|k| k.kind == "text")
            .unwrap()
            .withheld
            .is_none());
        let json = serde_json::to_value(&caps).unwrap();
        assert_eq!(json["kinds"][0]["kind"], "text");
        assert!(json["kinds"][0]["withheld"].is_null());
    }
}
