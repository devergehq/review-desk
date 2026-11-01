//! The developer's request for a fresh session on a review whose session is gone.
//! Migration 23, DEV-1142.
//!
//! **Review Desk starts nothing.** It records that the developer asked; the Locus dispatcher
//! reads it as the `fresh_session_requested` work kind, asks Allele itself whether the old
//! session is gone, and either starts one — which links a new session id, and that answers
//! the request — or declines with a reason, which the page shows.
//!
//! | Verb | Who | Through |
//! |---|---|---|
//! | [`Store::request_fresh_session`] | the developer | the dashboard only |
//! | [`Store::decline_fresh_session`] | a dispatcher | `review-desk review fresh-session --declined` |
//! | answering it | a new session | `review-desk review link --session`, which deletes the request |

use rusqlite::{params, OptionalExtension};

use crate::error::{Result, StoreError};
use crate::model::FreshSessionRequest;
use crate::Store;

impl Store {
    /// The developer asks for a fresh session. Asking again replaces the request.
    ///
    /// **Refused on a review that is out of play** — dismissed, merged, closed — for the
    /// reason a send-back is: nothing would act on it, and the page would say it is waiting
    /// for ever. Whether the old session is gone is not this record's to judge: the server
    /// asks Allele before calling this, and the dispatcher asks again before acting.
    pub fn request_fresh_session(&self, review_id: i64) -> Result<FreshSessionRequest> {
        self.get_review(review_id)?;
        self.refuse_if_dismissed(review_id, "starting a fresh session for it")?;
        self.conn.execute(
            "INSERT INTO fresh_session_requests (review_id, requested_at)
             VALUES (?1, ?2)
             ON CONFLICT (review_id) DO UPDATE SET
                 requested_at = excluded.requested_at,
                 declined_at = NULL,
                 declined_reason = NULL",
            params![review_id, self.now()],
        )?;
        self.mark_review_active(review_id)?;
        self.fresh_session_request(review_id)?
            .ok_or(StoreError::NotFound {
                entity: "fresh session request",
                id: review_id.to_string(),
            })
    }

    /// A dispatcher looked and did not start one, and says why.
    pub fn decline_fresh_session(
        &self,
        review_id: i64,
        reason: &str,
    ) -> Result<FreshSessionRequest> {
        let reason = reason.trim();
        if reason.is_empty() {
            return Err(StoreError::InvalidValue {
                field: "declined",
                value: String::new(),
                expected: "why no session was started, for the developer to read".to_string(),
            });
        }
        self.get_review(review_id)?;
        let changed = self.conn.execute(
            "UPDATE fresh_session_requests SET declined_at = ?2, declined_reason = ?3
              WHERE review_id = ?1",
            params![review_id, self.now(), reason],
        )?;
        if changed == 0 {
            return Err(StoreError::NotFound {
                entity: "fresh session request",
                id: review_id.to_string(),
            });
        }
        self.fresh_session_request(review_id)?
            .ok_or(StoreError::NotFound {
                entity: "fresh session request",
                id: review_id.to_string(),
            })
    }

    /// The request on this review, if there is one, waiting or declined.
    pub fn fresh_session_request(&self, review_id: i64) -> Result<Option<FreshSessionRequest>> {
        Ok(self
            .conn
            .query_row(
                "SELECT review_id, requested_at, declined_at, declined_reason
                   FROM fresh_session_requests WHERE review_id = ?1",
                params![review_id],
                |row| {
                    Ok(FreshSessionRequest {
                        review_id: row.get(0)?,
                        requested_at: row.get(1)?,
                        declined_at: row.get(2)?,
                        declined_reason: row.get(3)?,
                    })
                },
            )
            .optional()?)
    }
}
