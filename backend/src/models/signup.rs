use crate::schema::tables::event_signups;
use serde::{Deserialize, Serialize};

/// Metadata about a signed up attendee of an event
#[derive(Queryable, Serialize, Deserialize, Debug, PartialEq)]
pub struct Signup {
    pub id: i32,
    pub event: i32,
    pub name: String,
    pub email: String,
}

#[derive(Insertable, Serialize, Deserialize, Debug, PartialEq)]
#[diesel(table_name = event_signups)]
pub struct NewSignup {
    pub event: i32,
    pub name: String,
    pub email: String,
}
