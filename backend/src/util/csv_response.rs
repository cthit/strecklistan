use rocket::http::ContentType;
use rocket::{Request, Response};
use rocket::response::Responder;

pub struct CsvResponse<'a> {
    data: Vec<u8>,
    filename: &'a str,
}

impl <'a> CsvResponse<'a> {
    pub fn new(data: Vec<u8>, filename: &'a str) -> Self {
        CsvResponse { data, filename }
    }
}

impl<'r, 'a> Responder<'r, 'static> for CsvResponse<'a> {
    fn respond_to(self, _request: &'_ Request<'_>) -> rocket::response::Result<'static> {
        Response::build()
            .header(ContentType::CSV)
            .raw_header(
                "Content-Disposition",
                format!("attachment; filename=\"{}\"", self.filename),
            )
            .sized_body(self.data.len(), std::io::Cursor::new(self.data))
            .ok()
    }
}