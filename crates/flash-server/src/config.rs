//! Environment-based configuration. All public URLs derive from base_url.

use std::net::SocketAddr;
use std::path::PathBuf;

/// Outbound email (Resend). Present => self-serve signup is enabled.
#[derive(Debug, Clone)]
pub struct MailConfig {
    pub resend_api_key: String,
    /// Verified sender, e.g. "Flash <no-reply@flash.example.com>".
    pub from: String,
}

/// An S3-compatible bucket for media blobs (AWS S3, Cloudflare R2, MinIO,
/// Backblaze B2, …). Absent => blobs live on local disk under
/// data_dir/media, which is what most installs want. All four must be set
/// together.
#[derive(Debug, Clone)]
pub struct MediaR2Config {
    /// Scheme + host of the S3 endpoint, e.g. https://s3.us-east-1.amazonaws.com
    pub endpoint: String,
    pub bucket: String,
    pub access_key_id: String,
    pub secret_access_key: String,
}

/// Which model API every model-backed feature talks through. The value
/// names an API, not a vendor: `openai` is any endpoint speaking the
/// OpenAI Chat Completions protocol, hosted or local; `gemini` is the
/// native adapter for that API, which also takes video, video links and
/// file uploads. Each is one `ai::provider::LlmProvider` implementation
/// behind `ai::build_provider`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiProviderKind {
    Openai,
    Gemini,
}

impl AiProviderKind {
    pub const ALL: &'static [AiProviderKind] = &[AiProviderKind::Openai, AiProviderKind::Gemini];

    pub fn as_str(self) -> &'static str {
        match self {
            AiProviderKind::Openai => "openai",
            AiProviderKind::Gemini => "gemini",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|k| k.as_str().eq_ignore_ascii_case(s.trim()))
    }

    /// The model that reads and writes cards, and the one that grades
    /// spoken answers, when the operator names neither: the adapter's
    /// own choice, or None where the endpoint decides what it serves and
    /// the operator has to say.
    pub fn default_models(self) -> Option<(&'static str, &'static str)> {
        match self {
            AiProviderKind::Openai => None,
            AiProviderKind::Gemini => Some((
                crate::ai::gemini::DEFAULT_MODEL,
                crate::ai::gemini::DEFAULT_GRADER_MODEL,
            )),
        }
    }

    /// Whether the endpoint's address comes from the operator.
    fn needs_base_url(self) -> bool {
        matches!(self, AiProviderKind::Openai)
    }

    /// Whether an explicitly empty key is allowed: a local endpoint may
    /// need none.
    fn allows_empty_key(self) -> bool {
        matches!(self, AiProviderKind::Openai)
    }
}

/// How the generic adapter asks for the reply's shape. `Schema` sends
/// the JSON Schema in strict mode (`response_format: json_schema`);
/// `Json` asks for a JSON object and states the schema in the prompt,
/// for endpoints without `json_schema`; `Plain` sends no
/// `response_format` at all and reads the JSON out of the text, for
/// endpoints that ignore the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AiOutput {
    #[default]
    Schema,
    Json,
    Plain,
}

impl AiOutput {
    pub const ALL: &'static [AiOutput] = &[AiOutput::Schema, AiOutput::Json, AiOutput::Plain];

    pub fn as_str(self) -> &'static str {
        match self {
            AiOutput::Schema => "schema",
            AiOutput::Json => "json",
            AiOutput::Plain => "plain",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|v| v.as_str().eq_ignore_ascii_case(s.trim()))
    }
}

/// Card generation and spoken-answer grading through a model API.
/// Absent (no `FLASH_AI_PROVIDER`) => the generate pages and voice
/// grading are off and nothing in the binary ever calls out. Which
/// inputs are offered is the provider's to say and the operator's to
/// narrow, since a key may be on a plan that prices video and audio
/// differently from text.
#[derive(Debug, Clone)]
pub struct AiConfig {
    /// FLASH_AI_PROVIDER: which API. The switch for the whole feature.
    pub provider: AiProviderKind,
    /// FLASH_AI_BASE_URL: the endpoint's base (`https://host/v1`), for
    /// the providers whose address the operator gives; None for an
    /// adapter that knows its host.
    pub base_url: Option<String>,
    /// FLASH_AI_API_KEY: the key every call is made with. May be empty
    /// where the provider allows it (a local endpoint).
    pub api_key: String,
    /// FLASH_AI_MODEL: extraction and card writing.
    pub model: String,
    /// FLASH_AI_GRADER_MODEL: grading spoken answers and answering
    /// follow-up questions, where a smaller model does.
    pub grader_model: String,
    /// FLASH_AI_OUTPUT: how the generic adapter asks for the reply's
    /// shape; the native adapters have one way and ignore it.
    pub output: AiOutput,
    /// FLASH_AI_INPUTS: the source kinds the server accepts, when the
    /// operator narrows them; None means everything the provider's
    /// model reads.
    pub inputs: Option<Vec<crate::policy::AiInput>>,
    /// FLASH_PDFIUM_PATH: the PDF renderer, required when `pdf` is
    /// listed and enabling `pdf` when nothing is listed. A file, or a
    /// directory holding the platform library.
    pub pdfium_path: Option<PathBuf>,
    /// FLASH_DOC_CONVERTER: a command template with `{input}` and
    /// `{outdir}` that turns an office document into a PDF. Unset means
    /// documents are refused.
    pub doc_converter: Option<String>,
    /// FLASH_DOC_CONVERTER_TIMEOUT_SECS: how long one conversion may run.
    pub doc_converter_timeout: std::time::Duration,
    /// FLASH_VOICE_INPUT: what clients send the grader for a spoken
    /// answer. The server accepts both shapes whatever this says; this is
    /// what it tells clients to prefer.
    pub voice_input: VoiceInput,
}

/// What a spoken answer arrives as. `Transcript` is the device's own
/// speech recognition, free and good at the words it knows. `Audio` is
/// the recording itself, which the grader listens to: right on
/// specialised terms and other languages at a small cost per card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VoiceInput {
    #[default]
    Transcript,
    Audio,
}

impl VoiceInput {
    pub const ALL: &'static [VoiceInput] = &[VoiceInput::Transcript, VoiceInput::Audio];

    pub fn as_str(self) -> &'static str {
        match self {
            VoiceInput::Transcript => "transcript",
            VoiceInput::Audio => "audio",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|v| v.as_str().eq_ignore_ascii_case(s.trim()))
    }
}

#[cfg(test)]
mod voice_input_tests {
    use super::VoiceInput;

    #[test]
    fn the_flag_parses_its_two_values_and_nothing_else() {
        assert_eq!(VoiceInput::parse("audio"), Some(VoiceInput::Audio));
        assert_eq!(
            VoiceInput::parse(" Transcript "),
            Some(VoiceInput::Transcript)
        );
        assert_eq!(VoiceInput::parse("clip"), None);
        assert_eq!(VoiceInput::default(), VoiceInput::Transcript);
        for v in VoiceInput::ALL {
            assert_eq!(VoiceInput::parse(v.as_str()), Some(*v));
        }
    }
}

const DEFAULT_DOC_CONVERTER_TIMEOUT_SECS: u64 = 120;

/// The variables that only mean something once a provider is named.
const AI_DEPENDENT_VARS: &[&str] = &[
    "FLASH_AI_BASE_URL",
    "FLASH_AI_API_KEY",
    "FLASH_AI_MODEL",
    "FLASH_AI_GRADER_MODEL",
    "FLASH_AI_OUTPUT",
    "FLASH_AI_INPUTS",
    "FLASH_DOC_CONVERTER",
    "FLASH_VOICE_INPUT",
];

impl AiConfig {
    fn from_env() -> Result<Option<Self>, String> {
        Self::from_vars(|name| std::env::var(name).ok())
    }

    /// The configuration read through `get`, which answers with the raw
    /// value of a variable or None: the process environment in the
    /// binary, a map in the tests. A blank value counts as unset, except
    /// for the key, where an operator may mean "none".
    fn from_vars(get: impl Fn(&str) -> Option<String>) -> Result<Option<Self>, String> {
        let opt = |name: &str| {
            get(name)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        let Some(name) = opt("FLASH_AI_PROVIDER") else {
            for dependent in AI_DEPENDENT_VARS {
                if opt(dependent).is_some() {
                    return Err(format!("{dependent} is set but FLASH_AI_PROVIDER is not"));
                }
            }
            return Ok(None);
        };
        let provider = AiProviderKind::parse(&name).ok_or_else(|| {
            let known: Vec<&str> = AiProviderKind::ALL.iter().map(|k| k.as_str()).collect();
            format!(
                "FLASH_AI_PROVIDER: unknown provider {name:?} (expected {})",
                known.join(", ")
            )
        })?;
        let api_key = match get("FLASH_AI_API_KEY").map(|v| v.trim().to_string()) {
            None => {
                return Err(
                    "FLASH_AI_API_KEY is not set (the key the provider is called with; for \
                     FLASH_AI_PROVIDER=openai an empty value is allowed for a local endpoint \
                     that needs none)"
                        .to_string(),
                )
            }
            Some(key) if key.is_empty() && !provider.allows_empty_key() => {
                return Err("FLASH_AI_API_KEY is empty".to_string())
            }
            Some(key) => key,
        };
        let base_url = if provider.needs_base_url() {
            let raw = opt("FLASH_AI_BASE_URL").ok_or_else(|| {
                format!(
                    "FLASH_AI_BASE_URL is required for FLASH_AI_PROVIDER={}: the endpoint's \
                     base in the shape https://host/v1",
                    provider.as_str()
                )
            })?;
            let url = raw.trim_end_matches('/').to_string();
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                return Err("FLASH_AI_BASE_URL must start with http:// or https://".to_string());
            }
            if url::Url::parse(&url)
                .ok()
                .and_then(|u| u.host_str().map(str::to_string))
                .is_none()
            {
                return Err("FLASH_AI_BASE_URL is not a URL with a host".to_string());
            }
            Some(url)
        } else {
            None
        };
        let defaults = provider.default_models();
        let model = match (opt("FLASH_AI_MODEL"), defaults) {
            (Some(model), _) => model,
            (None, Some((model, _))) => model.to_string(),
            (None, None) => {
                return Err(format!(
                    "FLASH_AI_MODEL is required for FLASH_AI_PROVIDER={}: the model name the \
                     endpoint serves",
                    provider.as_str()
                ))
            }
        };
        let grader_model = match (opt("FLASH_AI_GRADER_MODEL"), defaults) {
            (Some(grader), _) => grader,
            (None, Some((_, grader))) => grader.to_string(),
            (None, None) => model.clone(),
        };
        let output = match opt("FLASH_AI_OUTPUT") {
            None => AiOutput::default(),
            Some(raw) => AiOutput::parse(&raw).ok_or_else(|| {
                let known: Vec<&str> = AiOutput::ALL.iter().map(|v| v.as_str()).collect();
                format!(
                    "FLASH_AI_OUTPUT: unknown value {raw:?} (expected {})",
                    known.join(", ")
                )
            })?,
        };
        let listed: Vec<String> = opt("FLASH_AI_INPUTS")
            .map(|raw| {
                raw.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let inputs = if listed.is_empty() {
            None
        } else {
            let mut inputs = Vec::new();
            for token in listed {
                let Some(input) = crate::policy::AiInput::parse(&token) else {
                    return Err(format!(
                        "FLASH_AI_INPUTS: unknown input {token:?} (expected text, pdf, image, \
                         youtube, video, audio)"
                    ));
                };
                if !inputs.contains(&input) {
                    inputs.push(input);
                }
            }
            Some(inputs)
        };
        let pdfium_path = opt("FLASH_PDFIUM_PATH").map(PathBuf::from);
        if inputs
            .as_ref()
            .is_some_and(|list| list.contains(&crate::policy::AiInput::Pdf))
            && pdfium_path.is_none()
        {
            return Err(
                "FLASH_AI_INPUTS lists pdf but FLASH_PDFIUM_PATH is not set (see tools/fetch-pdfium)"
                    .to_string(),
            );
        }
        let doc_converter = opt("FLASH_DOC_CONVERTER");
        let doc_converter_timeout = match opt("FLASH_DOC_CONVERTER_TIMEOUT_SECS") {
            None => DEFAULT_DOC_CONVERTER_TIMEOUT_SECS,
            Some(raw) => raw
                .trim()
                .parse::<u64>()
                .ok()
                .filter(|s| *s > 0)
                .ok_or_else(|| {
                    "FLASH_DOC_CONVERTER_TIMEOUT_SECS must be a whole number of seconds".to_string()
                })?,
        };
        let voice_input = match opt("FLASH_VOICE_INPUT") {
            None => VoiceInput::default(),
            Some(raw) => VoiceInput::parse(&raw).ok_or_else(|| {
                format!("FLASH_VOICE_INPUT: unknown value {raw:?} (expected transcript or audio)")
            })?,
        };
        Ok(Some(Self {
            provider,
            base_url,
            api_key,
            model,
            grader_model,
            output,
            inputs,
            pdfium_path,
            doc_converter,
            doc_converter_timeout: std::time::Duration::from_secs(doc_converter_timeout),
            voice_input,
        }))
    }
}

#[cfg(test)]
mod ai_config_tests {
    use super::*;
    use std::collections::HashMap;

    fn read(vars: &[(&str, &str)]) -> Result<Option<AiConfig>, String> {
        let map: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        AiConfig::from_vars(|name| map.get(name).cloned())
    }

    #[test]
    fn no_provider_means_the_feature_is_off_and_a_stray_setting_is_an_error() {
        assert!(read(&[]).unwrap().is_none());
        assert!(read(&[("FLASH_AI_PROVIDER", "  ")]).unwrap().is_none());
        for name in AI_DEPENDENT_VARS {
            let err = read(&[(name, "x")]).unwrap_err();
            assert_eq!(err, format!("{name} is set but FLASH_AI_PROVIDER is not"));
        }
        // The renderer's path is not a dependent: it is set on the
        // hosted image whether or not a model is.
        assert!(read(&[("FLASH_PDFIUM_PATH", "/lib")]).unwrap().is_none());
    }

    #[test]
    fn an_unknown_provider_lists_the_known_ones() {
        let err = read(&[("FLASH_AI_PROVIDER", "llama")]).unwrap_err();
        assert_eq!(
            err,
            "FLASH_AI_PROVIDER: unknown provider \"llama\" (expected openai, gemini)"
        );
    }

    #[test]
    fn the_native_adapter_needs_a_key_and_brings_its_own_models() {
        let err = read(&[("FLASH_AI_PROVIDER", "gemini")]).unwrap_err();
        assert!(err.starts_with("FLASH_AI_API_KEY is not set"), "{err}");
        assert_eq!(
            read(&[("FLASH_AI_PROVIDER", "gemini"), ("FLASH_AI_API_KEY", " ")]).unwrap_err(),
            "FLASH_AI_API_KEY is empty"
        );
        let config = read(&[
            ("FLASH_AI_PROVIDER", "Gemini"),
            ("FLASH_AI_API_KEY", "k"),
            ("FLASH_AI_BASE_URL", "https://ignored.example/v1"),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(config.provider, AiProviderKind::Gemini);
        assert_eq!(config.api_key, "k");
        assert_eq!(config.base_url, None, "the adapter knows its host");
        assert_eq!(config.model, crate::ai::gemini::DEFAULT_MODEL);
        assert_eq!(config.grader_model, crate::ai::gemini::DEFAULT_GRADER_MODEL);
        assert_eq!(config.output, AiOutput::Schema);
        let named = read(&[
            ("FLASH_AI_PROVIDER", "gemini"),
            ("FLASH_AI_API_KEY", "k"),
            ("FLASH_AI_MODEL", "m"),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(named.model, "m");
        assert_eq!(
            named.grader_model,
            crate::ai::gemini::DEFAULT_GRADER_MODEL,
            "the grader keeps its default when only the model is named"
        );
    }

    #[test]
    fn the_generic_adapter_needs_an_address_and_a_model_and_takes_an_empty_key() {
        let base = [("FLASH_AI_PROVIDER", "openai"), ("FLASH_AI_API_KEY", "k")];
        let err = read(&base).unwrap_err();
        assert_eq!(
            err,
            "FLASH_AI_BASE_URL is required for FLASH_AI_PROVIDER=openai: the endpoint's base \
             in the shape https://host/v1"
        );
        let mut vars = base.to_vec();
        vars.push(("FLASH_AI_BASE_URL", "localhost:11434/v1"));
        assert_eq!(
            read(&vars).unwrap_err(),
            "FLASH_AI_BASE_URL must start with http:// or https://"
        );
        vars[2] = ("FLASH_AI_BASE_URL", "http://:9/v1");
        assert_eq!(
            read(&vars).unwrap_err(),
            "FLASH_AI_BASE_URL is not a URL with a host"
        );
        vars[2] = ("FLASH_AI_BASE_URL", "http://127.0.0.1:11434/v1/");
        assert_eq!(
            read(&vars).unwrap_err(),
            "FLASH_AI_MODEL is required for FLASH_AI_PROVIDER=openai: the model name the \
             endpoint serves"
        );
        vars.push(("FLASH_AI_MODEL", "local-model"));
        let config = read(&vars).unwrap().unwrap();
        assert_eq!(config.provider, AiProviderKind::Openai);
        assert_eq!(
            config.base_url.as_deref(),
            Some("http://127.0.0.1:11434/v1"),
            "the trailing slash is dropped"
        );
        assert_eq!(config.model, "local-model");
        assert_eq!(
            config.grader_model, "local-model",
            "the grader defaults to the model"
        );
        assert_eq!(config.output, AiOutput::Schema);

        vars[1] = ("FLASH_AI_API_KEY", "");
        assert_eq!(
            read(&vars).unwrap().unwrap().api_key,
            "",
            "an explicitly empty key is allowed for a local endpoint"
        );
        vars.remove(1);
        assert!(read(&vars)
            .unwrap_err()
            .starts_with("FLASH_AI_API_KEY is not set"));

        let mut full = vec![
            ("FLASH_AI_PROVIDER", "openai"),
            ("FLASH_AI_API_KEY", "k"),
            ("FLASH_AI_BASE_URL", "https://api.example.com/v1"),
            ("FLASH_AI_MODEL", "m"),
            ("FLASH_AI_GRADER_MODEL", "g"),
            ("FLASH_AI_OUTPUT", "json"),
        ];
        let config = read(&full).unwrap().unwrap();
        assert_eq!(config.grader_model, "g");
        assert_eq!(config.output, AiOutput::Json);
        full[5] = ("FLASH_AI_OUTPUT", "Plain");
        assert_eq!(read(&full).unwrap().unwrap().output, AiOutput::Plain);
        full[5] = ("FLASH_AI_OUTPUT", "xml");
        assert_eq!(
            read(&full).unwrap_err(),
            "FLASH_AI_OUTPUT: unknown value \"xml\" (expected schema, json, plain)"
        );
    }

    #[test]
    fn the_rest_of_the_group_reads_as_before() {
        let mut vars = vec![
            ("FLASH_AI_PROVIDER", "gemini"),
            ("FLASH_AI_API_KEY", "k"),
            ("FLASH_AI_INPUTS", "text, image,text"),
            ("FLASH_VOICE_INPUT", "audio"),
            ("FLASH_DOC_CONVERTER_TIMEOUT_SECS", "30"),
        ];
        let config = read(&vars).unwrap().unwrap();
        assert_eq!(
            config.inputs,
            Some(vec![
                crate::policy::AiInput::Text,
                crate::policy::AiInput::Image
            ])
        );
        assert_eq!(config.voice_input, VoiceInput::Audio);
        assert_eq!(config.doc_converter_timeout.as_secs(), 30);
        vars[2] = ("FLASH_AI_INPUTS", "text,pdf");
        assert!(read(&vars)
            .unwrap_err()
            .starts_with("FLASH_AI_INPUTS lists pdf but FLASH_PDFIUM_PATH is not set"));
        vars[2] = ("FLASH_AI_INPUTS", "scrolls");
        assert!(read(&vars)
            .unwrap_err()
            .starts_with("FLASH_AI_INPUTS: unknown input \"scrolls\""));
    }
}

/// The natural-voice sidecar (see tools/flash-tts): cards are read in
/// a synthesized voice made a few cards ahead of the study queue.
/// Absent => the apps read with the device's own voice and every voice
/// audio route says the feature is off.
#[derive(Debug, Clone)]
pub struct TtsConfig {
    /// FLASH_TTS_URL: the sidecar's origin, e.g. http://127.0.0.1:8880.
    pub url: String,
    /// FLASH_TTS_VOICE_DEFAULT: the voice an account starts with.
    pub default_voice: String,
    /// FLASH_TTS_TIMEOUT_SECS: how long one synthesis may take. Long
    /// enough for a long back on a busy single core.
    pub timeout: std::time::Duration,
}

const DEFAULT_TTS_TIMEOUT_SECS: u64 = 90;

impl TtsConfig {
    fn from_env() -> Result<Option<Self>, String> {
        let Some(url) = env_opt("FLASH_TTS_URL") else {
            for name in ["FLASH_TTS_VOICE_DEFAULT", "FLASH_TTS_TIMEOUT_SECS"] {
                if env_opt(name).is_some() {
                    return Err(format!("{name} is set but FLASH_TTS_URL is not"));
                }
            }
            return Ok(None);
        };
        let url = url.trim().trim_end_matches('/').to_string();
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err("FLASH_TTS_URL must start with http:// or https://".to_string());
        }
        let default_voice = env_opt("FLASH_TTS_VOICE_DEFAULT")
            .unwrap_or_else(|| flash_core::settings::DEFAULT_VOICE.to_string());
        if crate::tts::voices::find(&default_voice).is_none() {
            return Err(format!(
                "FLASH_TTS_VOICE_DEFAULT: unknown voice {default_voice:?}"
            ));
        }
        let timeout = match env_opt("FLASH_TTS_TIMEOUT_SECS") {
            None => DEFAULT_TTS_TIMEOUT_SECS,
            Some(raw) => raw
                .trim()
                .parse::<u64>()
                .ok()
                .filter(|s| *s > 0)
                .ok_or_else(|| {
                    "FLASH_TTS_TIMEOUT_SECS must be a whole number of seconds".to_string()
                })?,
        };
        Ok(Some(Self {
            url,
            default_voice,
            timeout: std::time::Duration::from_secs(timeout),
        }))
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    /// Loopback by default: a reverse proxy or tunnel in front terminates TLS.
    pub bind: SocketAddr,
    /// FLASH_CLIENT_IP_HEADER: the request header that carries the real
    /// client address when a proxy sits in front (`cf-connecting-ip`,
    /// `x-real-ip`, …). Unset => the TCP peer address is used. Set it only
    /// when nothing but that proxy can reach the port, since any header
    /// is forgeable by a direct caller.
    pub client_ip_header: Option<String>,
    /// Canonical public origin, e.g. https://cards.example.com
    pub base_url: String,
    /// FLASH_SUPPORT_EMAIL: printed wherever a page shows a contact
    /// address. Unset hides those lines.
    pub support_email: Option<String>,
    pub data_dir: PathBuf,
    pub mail: Option<MailConfig>,
    /// FLASH_DEV_MAIL_LOG=1: a mailer without a provider. Every message
    /// and its link go to the log instead (dev / self-host convenience).
    pub dev_mail_log: bool,
    pub media_r2: Option<MediaR2Config>,
    pub ai: Option<AiConfig>,
    /// Natural voice through the sidecar; None reads with device voices.
    pub tts: Option<TtsConfig>,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let bind = std::env::var("FLASH_BIND")
            .unwrap_or_else(|_| "127.0.0.1:8437".to_string())
            .parse()
            .map_err(|e| format!("FLASH_BIND: {e}"))?;
        let base_url = std::env::var("FLASH_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:8437".to_string())
            .trim_end_matches('/')
            .to_string();
        let support_email = env_opt("FLASH_SUPPORT_EMAIL");
        let data_dir =
            PathBuf::from(std::env::var("FLASH_DATA_DIR").unwrap_or_else(|_| "./data".to_string()));

        let mail = match (env_opt("RESEND_API_KEY"), env_opt("FLASH_EMAIL_FROM")) {
            (Some(resend_api_key), Some(from)) => Some(MailConfig {
                resend_api_key,
                from,
            }),
            (None, None) => None,
            _ => return Err("RESEND_API_KEY and FLASH_EMAIL_FROM must be set together".to_string()),
        };
        let dev_mail_log = std::env::var("FLASH_DEV_MAIL_LOG").is_ok_and(|v| v == "1");

        let r2_vars = [
            env_opt("FLASH_MEDIA_R2_ENDPOINT"),
            env_opt("FLASH_MEDIA_R2_BUCKET"),
            env_opt("FLASH_MEDIA_R2_ACCESS_KEY_ID"),
            env_opt("FLASH_MEDIA_R2_SECRET_ACCESS_KEY"),
        ];
        let media_r2 = match r2_vars {
            [Some(endpoint), Some(bucket), Some(access_key_id), Some(secret_access_key)] => {
                Some(MediaR2Config {
                    endpoint,
                    bucket,
                    access_key_id,
                    secret_access_key,
                })
            }
            [None, None, None, None] => None,
            _ => {
                return Err("FLASH_MEDIA_R2_ENDPOINT, FLASH_MEDIA_R2_BUCKET, \
                            FLASH_MEDIA_R2_ACCESS_KEY_ID, and FLASH_MEDIA_R2_SECRET_ACCESS_KEY \
                            must be set together"
                    .to_string())
            }
        };

        // Header names are case-insensitive on the wire; http::HeaderMap
        // wants them lowercase for lookup.
        let client_ip_header = env_opt("FLASH_CLIENT_IP_HEADER").map(|h| h.to_ascii_lowercase());
        let ai = AiConfig::from_env()?;
        let tts = TtsConfig::from_env()?;

        Ok(Self {
            bind,
            client_ip_header,
            base_url,
            support_email,
            data_dir,
            mail,
            dev_mail_log,
            media_r2,
            ai,
            tts,
        })
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("flash.db")
    }
}

/// An env var, with blank treated as unset.
pub fn env_opt(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// A comma-separated list; unset or blank => empty.
pub fn env_list(name: &str) -> Vec<String> {
    env_opt(name)
        .map(|v| {
            v.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}
