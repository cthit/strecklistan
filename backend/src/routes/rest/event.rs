use crate::database::DatabasePool;
use crate::database::event::{get_event_ws, get_event_ws_range};
use crate::models::event::EventWithSignups as EventWS;
use crate::util::ser::{Ser, SerAccept};
use crate::util::status_json::StatusJson as SJ;
use rocket::{State, get};

#[get("/event/<id>")]
pub fn get_event(
    db_pool: &State<DatabasePool>,
    accept: SerAccept,
    id: i32,
) -> Result<Ser<EventWS>, SJ> {
    Ok(accept.ser(get_event_ws(db_pool.inner().get()?, id, true)?))
}

#[get("/events?<low>&<high>")]
pub fn get_event_range(
    db_pool: &State<DatabasePool>,
    accept: SerAccept,
    low: i64,
    high: i64,
) -> Result<Ser<Vec<EventWS>>, SJ> {
    Ok(accept.ser(get_event_ws_range(db_pool.inner().get()?, low, high, true)?))
}
