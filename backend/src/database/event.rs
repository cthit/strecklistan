use crate::database::DatabaseConn;
use crate::models::event::EventWithSignups as EventWS;
use chrono::Local;
use diesel::prelude::*;
use diesel::result::QueryResult as Result;
use diesel::sql_types::Bool;

pub fn get_event_ws(
    mut connection: DatabaseConn,
    id: i32,
    published_only: bool,
) -> Result<EventWS> {
    use crate::schema::views::events_with_signups::dsl::{events_with_signups, published};

    events_with_signups
        .find(id)
        .filter(published.eq(true).or::<_, Bool>(!published_only))
        .first(&mut connection)
}

pub fn get_event_ws_range(
    mut connection: DatabaseConn,
    low: i64,
    high: i64,
    published_only: bool,
) -> Result<Vec<EventWS>> {
    use crate::schema::views::events_with_signups::dsl::*;

    assert!(high > low);

    let now = Local::now().naive_local();

    let mut previous: Vec<EventWS> = if low < 0 {
        events_with_signups
            .filter(end_time.le(now))
            .filter(published.eq(true).or::<_, Bool>(!published_only))
            .order_by(start_time.desc())
            .limit(-low)
            .load(&mut connection)?
    } else {
        Vec::new()
    };

    let mut upcoming: Vec<EventWS> = if high > 0 {
        events_with_signups
            .filter(end_time.gt(now))
            .filter(published.eq(true).or::<_, Bool>(!published_only))
            .order_by(start_time.asc())
            .limit(high)
            .load(&mut connection)?
    } else {
        Vec::new()
    };

    if high < 0 {
        if (-high) as usize >= previous.len() {
            previous = Vec::new();
        } else {
            previous.drain(..(-high as usize));
        }
    }

    if low > 0 {
        if low as usize >= upcoming.len() {
            upcoming = Vec::new();
        } else {
            upcoming.drain(..(low as usize));
        }
    }

    upcoming.reverse();

    upcoming.append(&mut previous);
    Ok(upcoming)
}
