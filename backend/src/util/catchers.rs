use crate::util::StatusJson;
use rocket::http::Status;
use rocket::{Catcher, Request, catch, catchers};

pub fn catchers() -> Vec<Catcher> {
    catchers![not_found, unauthorized, bad_request,]
}

#[catch(404)]
pub fn not_found(_: &Request) -> StatusJson {
    StatusJson {
        status: Status::NotFound,
        description: "Route Not Found".into(),
    }
}

#[catch(401)]
pub fn unauthorized(_: &Request) -> StatusJson {
    Status::Unauthorized.into()
}

#[catch(400)]
pub fn bad_request(_: &Request) -> StatusJson {
    Status::BadRequest.into()
}
