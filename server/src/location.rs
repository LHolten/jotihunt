use axum::http::StatusCode;
use jotihunt_shared::{
    ModifyLocation,
    domain::{Fox, FoxKey},
};
use rust_query::{FromExpr, TableRow, Transaction};

use crate::{
    AppState,
    db::{Location, LocationRow, Schema},
    encode_broadcast,
};

/// Insert a new location, fails with 409 if it already exists.
pub async fn create_location(state: &'static AppState, key: FoxKey, fox: Fox) -> StatusCode {
    state
        .db
        .transaction_mut_ok(move |txn| {
            let new = Location {
                day: key.day.clone(),
                time: key.time.clone(),
                fox_name: key.fox_name.clone(),
                latitude: fox.latitude.clone(),
                longitude: fox.longitude.clone(),
            };
            if txn.insert(new).is_err() {
                return StatusCode::CONFLICT;
            }
            let _ = state
                .live_locations
                .send(encode_broadcast(&key, Some(&fox)));
            StatusCode::CREATED
        })
        .await
}

/// Replace the location `edit.old` with `edit.new`.
/// Fails with 404 if it doesn't exist and 409 if the current location is not `edit.old`.
pub async fn modify_location(
    state: &'static AppState,
    key: FoxKey,
    edit: ModifyLocation,
) -> StatusCode {
    state
        .db
        .transaction_mut_ok(move |txn| {
            let ModifyLocation { old, new } = edit;
            let Some((row, current)) = find_location(txn, &key) else {
                return StatusCode::NOT_FOUND;
            };
            if current != old {
                return StatusCode::CONFLICT;
            }
            txn.scope(|txn| {
                let mut location = txn.mutable(row);
                location.latitude = new.latitude.clone();
                location.longitude = new.longitude.clone();
            });
            let _ = state
                .live_locations
                .send(encode_broadcast(&key, Some(&new)));
            StatusCode::OK
        })
        .await
}

/// Delete the location if it is `old`.
/// Fails with 404 if it doesn't exist and 409 if the current location is not `old`.
pub async fn delete_location(state: &'static AppState, key: FoxKey, old: Fox) -> StatusCode {
    state
        .db
        .transaction_mut_ok(move |txn| {
            let Some((row, current)) = find_location(txn, &key) else {
                return StatusCode::NOT_FOUND;
            };
            if current != old {
                return StatusCode::CONFLICT;
            }
            txn.downgrade().delete_ok(row);
            let _ = state
                .live_locations
                .send(encode_broadcast(&key, None::<&Fox>));
            StatusCode::OK
        })
        .await
}

fn find_location(txn: &Transaction<Schema>, key: &FoxKey) -> Option<(TableRow<Location>, Fox)> {
    let row = txn.query_one(
        Location
            .day(key.day.clone())
            .time(key.time.clone())
            .fox_name(key.fox_name.clone()),
    )?;
    let (_, fox) = txn.query_one(LocationRow::from_expr(row)).into_parts();
    Some((row, fox))
}

#[cfg(test)]
mod tests {
    use rust_query::migration::Config;

    use super::*;
    use crate::leak;

    #[tokio::test]
    async fn location_edits() {
        let state = leak(AppState::new(Config::open_in_memory()));
        let mut changes = state.live_locations.subscribe();

        let key = FoxKey {
            day: "2026-10-05".to_owned(),
            time: "10:00".to_owned(),
            fox_name: "Alpha".to_owned(),
        };
        let a = Fox {
            latitude: "1".to_owned(),
            longitude: "2".to_owned(),
        };
        let b = Fox {
            latitude: "3".to_owned(),
            longitude: "4".to_owned(),
        };
        let k = || key.clone();
        let modify = |old: &Fox, new: &Fox| ModifyLocation {
            old: old.clone(),
            new: new.clone(),
        };

        let status = create_location(state, k(), a.clone()).await;
        assert_eq!(status, StatusCode::CREATED);
        let status = create_location(state, k(), b.clone()).await;
        assert_eq!(status, StatusCode::CONFLICT);
        let status = modify_location(state, k(), modify(&b, &a)).await;
        assert_eq!(status, StatusCode::CONFLICT);
        let status = modify_location(state, k(), modify(&a, &b)).await;
        assert_eq!(status, StatusCode::OK);
        let status = delete_location(state, k(), a.clone()).await;
        assert_eq!(status, StatusCode::CONFLICT);
        let status = delete_location(state, k(), b.clone()).await;
        assert_eq!(status, StatusCode::OK);
        let status = delete_location(state, k(), b.clone()).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let status = modify_location(state, k(), modify(&b, &a)).await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        let expected = [Some(&a), Some(&b), None].map(|fox| encode_broadcast(&key, fox));
        for bin in expected {
            assert_eq!(changes.try_recv().unwrap(), bin);
        }
        assert!(changes.try_recv().is_err());
    }
}
