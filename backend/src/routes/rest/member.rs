use crate::database::DatabasePool;
use crate::util::ser::{Ser, SerAccept};
use crate::util::status_json::StatusJson as SJ;
use diesel::prelude::*;
use rocket::serde::json::Json;
use rocket::{State, get, post};
use std::collections::HashMap;
use strecklistan_api::book_account::{BookAccountId, BookAccountType};
use strecklistan_api::member::{Member, MemberId, NewMember};

#[get("/members")]
pub fn get_members(
    db_pool: &State<DatabasePool>,
    accept: SerAccept,
) -> Result<Ser<HashMap<MemberId, Member>>, SJ> {
    let mut connection = db_pool.inner().get()?;
    use crate::schema::tables::members::dsl::*;

    Ok(accept.ser(
        members
            .load(&mut connection)?
            .into_iter()
            .map(|member: Member| (member.id, member))
            .collect(),
    ))
}

#[post("/add_member_with_book_account", data = "<data>")]
pub fn add_member_with_book_account(
    db_pool: &State<DatabasePool>,
    accept: SerAccept,
    data: Json<(NewMember, String)>,
) -> Result<Ser<(MemberId, BookAccountId)>, SJ> {
    let mut connection = db_pool.inner().get()?;

    let (new_member, account_name) = data.into_inner();

    connection.transaction::<_, SJ, _>(|connection| {
        let member_id = {
            use crate::schema::tables::members::dsl::*;

            diesel::insert_into(members)
                .values((
                    first_name.eq(&new_member.first_name),
                    last_name.eq(&new_member.last_name),
                    nickname.eq(&new_member.nickname),
                ))
                .returning(id)
                .get_result(connection)?
        };

        let acc_id = {
            use crate::schema::tables::book_accounts::dsl::*;

            diesel::insert_into(book_accounts)
                .values((
                    name.eq(&account_name),
                    account_type.eq(&BookAccountType::Liabilities),
                    creditor.eq(&Some(member_id)),
                ))
                .returning(id)
                .get_result(connection)?
        };

        Ok(accept.ser((member_id, acc_id)))
    })
}
