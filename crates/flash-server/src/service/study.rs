use flash_core::queue::StudyScope;
use flash_core::{CardId, DeckId, Rating, SessionId, UserId};
use flash_store::{richtext, StoreError};

use crate::service::{
    lookahead, CardFront, DiffSpan, Result, Reveal, ReviewOrigin, ServiceError, Services,
    SessionInfo, SessionSurface, StudyCard, SubmitResult,
};

/// 自选学习的服务端进度与当前卡片。
pub struct CustomStudyHead {
    pub mode: String,
    pub current: Option<CardId>,
    pub position: usize,
    pub total: usize,
}

impl Services {
    /// 按指定范围建立一次不受到期时间限制的学习会话。
    pub fn start_custom_study(
        &self,
        user: UserId,
        scope: StudyScope,
        mode: &str,
        random: bool,
        limit: u32,
        now: i64,
    ) -> Result<SessionId> {
        super::check_scope(&scope)?;
        if !matches!(mode, "review" | "practice") || limit == 0 {
            return Err(ServiceError::Malformed(
                "invalid custom study options".into(),
            ));
        }
        if let Some(deck) = scope.deck {
            self.store.get_deck_limits(user, deck)?;
        }
        let cards = self.store.custom_study_cards(user, &scope, random, limit)?;
        Ok(self
            .store
            .start_custom_study(
                user,
                &scope,
                mode,
                if random { "random" } else { "ordered" },
                &cards,
                now,
            )?)
    }

    /// 查询自选学习进度，题目顺序由会话保存。
    pub fn custom_study_head(&self, user: UserId, session: SessionId) -> Result<CustomStudyHead> {
        let (mode, cards, position) =
            self.store
                .custom_study_session(user, session, crate::service::now_ms())?;
        Ok(CustomStudyHead {
            mode,
            current: cards.get(position).copied(),
            position,
            total: cards.len(),
        })
    }

    /// 仅主动复习评分；自由练习推进进度但不写入 FSRS 状态。
    pub fn advance_custom_study(
        &self,
        user: UserId,
        session: SessionId,
        card: CardId,
        rating: Option<Rating>,
        now: i64,
    ) -> Result<()> {
        let head = self.custom_study_head(user, session)?;
        if head.current != Some(card) {
            return Err(ServiceError::Malformed("card is not current".into()));
        }
        if head.mode == "review" {
            let rating =
                rating.ok_or_else(|| ServiceError::Malformed("rating is required".into()))?;
            let settings = self.store.get_settings(user)?;
            let state = self.store.get_card_state(user, card)?;
            let goal = self.card_goal_window(user, &settings, card, now)?;
            let outcome = self
                .scheduler_for(&settings)?
                .review_toward(&state, rating, now, card.0, goal.as_ref())
                .map_err(|e| ServiceError::Scheduler(e.to_string()))?;
            self.store.advance_custom_study(
                user,
                session,
                card,
                Some(&outcome),
                state.last_review_ms,
                now,
            )?;
        } else {
            if rating.is_some() {
                return Err(ServiceError::Malformed(
                    "practice does not accept a rating".into(),
                ));
            }
            self.store
                .advance_custom_study(user, session, card, None, None, now)?;
        }
        Ok(())
    }
    pub fn start_session(
        &self,
        user: UserId,
        scope: StudyScope,
        now_ms: i64,
    ) -> Result<SessionInfo> {
        self.start(user, scope, None, now_ms)
    }

    /// `start_session` from a known surface, which the session keeps so
    /// it can be counted by where it was started; a study page loaded
    /// again reuses its session (`SessionSurface::reuse_ms`).
    pub fn start_session_from(
        &self,
        user: UserId,
        scope: StudyScope,
        surface: SessionSurface,
        now_ms: i64,
    ) -> Result<SessionInfo> {
        self.start(user, scope, Some(surface), now_ms)
    }

    fn start(
        &self,
        user: UserId,
        scope: StudyScope,
        surface: Option<SessionSurface>,
        now_ms: i64,
    ) -> Result<SessionInfo> {
        // A session scoped to a deck is a session on one of the user's
        // decks; a foreign id is not found, not an empty session.
        if let Some(deck) = scope.deck {
            self.store.get_deck_limits(user, deck)?;
        }
        let (queue, held_by_limit) = self.build_queue(user, &scope, now_ms)?;
        // Only an empty queue needs to know: a queue with cards has them.
        let scope_has_cards = !queue.is_empty() || self.store.scope_has_cards(user, &scope)?;
        let session_id = match surface {
            Some(s) => self
                .store
                .open_session(user, &scope, s.as_str(), s.reuse_ms(), now_ms)?,
            None => self.store.start_session(user, &scope, now_ms)?,
        };
        let settings = self.store.get_settings(user)?;
        let first_card = match queue.first() {
            Some(&id) => self.card_front(user, id)?,
            None => None,
        };
        Ok(SessionInfo {
            session_id,
            grading_mode: settings.grading_mode,
            cards_due: queue.len() as u32,
            held_by_limit,
            scope_has_cards,
            first_card,
            next_card_id: queue.get(1).copied(),
        })
    }

    /// Records a review and returns the next card in the same call: an
    /// assistant studying with someone makes one tool call per card.
    pub fn submit_review(
        &self,
        user: UserId,
        session: SessionId,
        card: CardId,
        rating: Rating,
        origin: ReviewOrigin<'_>,
        now_ms: i64,
    ) -> Result<SubmitResult> {
        if self.store.is_custom_study(user, session)? {
            return Err(ServiceError::Malformed(
                "custom study requires its own submission".into(),
            ));
        }
        let scope = self.store.get_session_scope(user, session)?;
        let settings = self.store.get_settings(user)?;
        let state = self.store.get_card_state(user, card)?;

        // Transport-level retries (Streamable HTTP clients may re-POST)
        // must not double-record: a repeat submission for a card reviewed
        // seconds ago is answered idempotently with the current queue.
        const RETRY_WINDOW_MS: i64 = 5_000;
        if state
            .last_review_ms
            .is_some_and(|t| (0..RETRY_WINDOW_MS).contains(&now_ms.saturating_sub(t)))
        {
            let (queue, held_by_limit) = self.build_queue(user, &scope, now_ms)?;
            let next = queue.iter().find(|&&id| id != card).or(queue.first());
            let next_card = match next {
                Some(&id) => self.card_front(user, id)?,
                None => None,
            };
            let next_card_id = lookahead(&queue, next.copied());
            return Ok(SubmitResult {
                remaining: queue.len() as u32,
                held_by_limit,
                next_card,
                next_card_id,
            });
        }

        let goal = self.card_goal_window(user, &settings, card, now_ms)?;
        let outcome = self
            .scheduler_for(&settings)?
            .review_toward(&state, rating, now_ms, card.0, goal.as_ref())
            .map_err(|e| ServiceError::Scheduler(e.to_string()))?;
        self.store.record_review_timed(
            user,
            card,
            &outcome,
            now_ms,
            origin.source,
            origin.client,
            Some(session),
            origin.answer_ms,
        )?;

        let (queue, held_by_limit) = self.build_queue(user, &scope, now_ms)?;
        // The just-reviewed card may already be due again (learning step);
        // don't hand it straight back.
        let next = queue.iter().find(|&&id| id != card).or(queue.first());
        let next_card = match next {
            Some(&id) => self.card_front(user, id)?,
            None => None,
        };
        let next_card_id = lookahead(&queue, next.copied());
        Ok(SubmitResult {
            remaining: queue.len() as u32,
            held_by_limit,
            next_card,
            next_card_id,
        })
    }

    /// How long until `card` comes back after each grade, Again to Easy,
    /// were it graded at `now_ms`. Records nothing.
    pub fn grade_preview(&self, user: UserId, card: CardId, now_ms: i64) -> Result<[i64; 4]> {
        let settings = self.store.get_settings(user)?;
        let state = self.store.get_card_state(user, card)?;
        let scheduler = self.scheduler_for(&settings)?;
        let goal = self.card_goal_window(user, &settings, card, now_ms)?;
        let mut out = [0; 4];
        let ratings = [Rating::Again, Rating::Hard, Rating::Good, Rating::Easy];
        for (slot, rating) in out.iter_mut().zip(ratings) {
            let outcome = scheduler
                .review_toward(&state, rating, now_ms, card.0, goal.as_ref())
                .map_err(|e| ServiceError::Scheduler(e.to_string()))?;
            *slot = outcome.due_after_ms - now_ms;
        }
        Ok(out)
    }

    /// `submit_review` for a surface that can fall behind another one
    /// driving the same session: the MCP study panel, open beside a chat
    /// where the model may grade the same card. A card no longer in the
    /// session's queue was graded elsewhere since the surface showed it,
    /// so it is not recorded twice; the answer is false with the queue as
    /// it stands, and the surface shows its first card.
    pub fn submit_review_if_queued(
        &self,
        user: UserId,
        session: SessionId,
        card: CardId,
        rating: Rating,
        origin: ReviewOrigin<'_>,
        now_ms: i64,
    ) -> Result<(bool, SubmitResult)> {
        let scope = self.store.get_session_scope(user, session)?;
        let (queue, held_by_limit) = self.build_queue(user, &scope, now_ms)?;
        if queue.contains(&card) {
            let result = self.submit_review(user, session, card, rating, origin, now_ms)?;
            return Ok((true, result));
        }
        let next = queue.first().copied();
        Ok((
            false,
            SubmitResult {
                remaining: queue.len() as u32,
                held_by_limit,
                next_card: match next {
                    Some(id) => self.card_front(user, id)?,
                    None => None,
                },
                next_card_id: lookahead(&queue, next),
            },
        ))
    }

    /// Where a session stands now, without grading anything: its queue's
    /// first card and the one after it. For a surface whose card went away
    /// under it (an edit that changed the note's type), so it goes on with
    /// the session instead of starting another.
    pub fn session_head(
        &self,
        user: UserId,
        session: SessionId,
        now_ms: i64,
    ) -> Result<SubmitResult> {
        let scope = self.store.get_session_scope(user, session)?;
        let (queue, held_by_limit) = self.build_queue(user, &scope, now_ms)?;
        let next = queue.first().copied();
        Ok(SubmitResult {
            remaining: queue.len() as u32,
            held_by_limit,
            next_card: match next {
                Some(id) => self.card_front(user, id)?,
                None => None,
            },
            next_card_id: lookahead(&queue, next),
        })
    }

    /// The front of the user's card as a study surface opens it, or None
    /// when the card is gone (deleted, or dropped by an edit of its note).
    pub fn study_front(&self, user: UserId, card: CardId) -> Result<Option<CardFront>> {
        self.card_front(user, card)
    }

    /// The card's back text, for reveal ("show me the answer").
    pub fn reveal(&self, user: UserId, card: CardId) -> Result<String> {
        Ok(self
            .store
            .get_card(user, card)?
            .ok_or(StoreError::NotFound("card"))?
            .back)
    }

    /// The study view of `front`, with `next` (the queue's following card)
    /// contributing its images to the prefetch list.
    pub fn study_card(
        &self,
        user: UserId,
        front: CardFront,
        next: Option<CardId>,
    ) -> Result<StudyCard> {
        let row = self
            .store
            .get_card(user, front.card_id)?
            .ok_or(StoreError::NotFound("card"))?;
        let preload_media = row
            .back_html
            .as_deref()
            .map(richtext::media_image_ids)
            .unwrap_or_default();
        let prefetch_media = match next {
            Some(id) => self.card_image_ids(user, id)?,
            None => Vec::new(),
        };
        let labeled = self.labeled_side(user, &row, flash_store::CardSide::Front)?;
        Ok(StudyCard {
            card_id: front.card_id,
            front: front.front,
            front_html: labeled.html.or(row.front_html),
            front_lang: labeled.lang,
            wants_typing: row.type_answer.is_some(),
            preload_media,
            prefetch_media,
            next_card: next,
            speech_language_hints: self.store.card_speech_hints(user, front.card_id)?,
        })
    }

    /// Every image id on both sides of a card, for prefetching.
    fn card_image_ids(&self, user: UserId, card: CardId) -> Result<Vec<i64>> {
        let Some(row) = self.store.get_card(user, card)? else {
            return Ok(Vec::new());
        };
        let mut ids = Vec::new();
        for html in [row.front_html.as_deref(), row.back_html.as_deref()]
            .into_iter()
            .flatten()
        {
            for id in richtext::media_image_ids(html) {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
        Ok(ids)
    }

    /// Reveal for the interactive surfaces: the back, plus the diff of
    /// what the user typed when the card asked for it.
    pub fn reveal_card(&self, user: UserId, card: CardId, typed: Option<&str>) -> Result<Reveal> {
        let row = self
            .store
            .get_card(user, card)?
            .ok_or(StoreError::NotFound("card"))?;
        let typed_diff = match (&row.type_answer, typed.map(str::trim)) {
            (Some(expected), Some(typed)) if !typed.is_empty() => Some(type_diff(expected, typed)),
            _ => None,
        };
        let front = self.labeled_side(user, &row, flash_store::CardSide::Front)?;
        let back = self.labeled_side(user, &row, flash_store::CardSide::Back)?;
        Ok(Reveal {
            card_id: row.id,
            front: row.front,
            front_html: front.html.or(row.front_html),
            front_lang: front.lang,
            back: row.back,
            back_html: back.html.or(row.back_html),
            back_lang: back.lang,
            typed_diff,
        })
    }

    /// A side of `row` as a page shows it: the language its labels say
    /// (empty when none does), and its HTML with each language's runs
    /// marked when it holds several (`labels::label_side`).
    pub fn labeled_side(
        &self,
        user: UserId,
        row: &flash_store::CardRow,
        side: flash_store::CardSide,
    ) -> Result<flash_store::labels::LabeledSide> {
        let picked = self.store.card_side_lang(user, row.id, side)?;
        let (html, text, blocks) = match side {
            flash_store::CardSide::Front => (&row.front_html, &row.front, &row.front_speech),
            flash_store::CardSide::Back => (&row.back_html, &row.back, &row.back_speech),
        };
        Ok(flash_store::labels::label_side(
            html.as_deref(),
            text,
            blocks.as_deref(),
            picked.as_deref(),
        ))
    }

    /// What a session was started on, so an empty queue can offer the
    /// right boost (that deck's, or the account's).
    pub fn session_scope(&self, user: UserId, session: SessionId) -> Result<StudyScope> {
        Ok(self.store.get_session_scope(user, session)?)
    }

    /// The tags on the user's live cards with their card counts, within
    /// one deck's subtree when given: what the study surfaces offer as a
    /// filter.
    pub fn list_tags(
        &self,
        user: UserId,
        deck: Option<DeckId>,
    ) -> Result<Vec<flash_store::TagCount>> {
        Ok(self.store.list_tags(user, deck)?)
    }

    /// Start of the user's current study day, for anything that runs on
    /// the same clock as the daily budgets.
    pub fn study_day_start(&self, user: UserId, now_ms: i64) -> Result<i64> {
        let settings = self.store.get_settings(user)?;
        Ok(Self::study_day_start_ms(&settings, now_ms))
    }
}

/// Anki-style typed-answer feedback: the typed string with each char
/// marked right/wrong by LCS alignment against the expected answer (which
/// the revealed back shows anyway). Expected chars the user missed are
/// simply skipped; extra typed chars are wrong; an empty result renders
/// as a single dash.
pub fn type_diff(expected: &str, typed: &str) -> Vec<DiffSpan> {
    let e: Vec<char> = flash_core::text::nfc(expected.trim())
        .chars()
        .take(200)
        .collect();
    let t: Vec<char> = flash_core::text::nfc(typed.trim())
        .chars()
        .take(200)
        .collect();
    let (n, m) = (e.len(), t.len());
    let mut dp = vec![vec![0u16; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if e[i] == t[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let mut spans: Vec<DiffSpan> = Vec::new();
    let push = |ch: char, ok: bool, spans: &mut Vec<DiffSpan>| match spans.last_mut() {
        Some(s) if s.ok == ok => s.text.push(ch),
        _ => spans.push(DiffSpan {
            text: ch.to_string(),
            ok,
        }),
    };
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if e[i] == t[j] {
            push(t[j], true, &mut spans);
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            i += 1; // expected char the user missed; the back shows it
        } else {
            push(t[j], false, &mut spans);
            j += 1;
        }
    }
    while j < m {
        push(t[j], false, &mut spans);
        j += 1;
    }
    if spans.is_empty() {
        spans.push(DiffSpan {
            text: "–".into(),
            ok: false,
        });
    }
    spans
}
