use crate::database::DatabasePool;
use crate::models::book_account as relational;
use crate::models::transaction::relational::Transaction;
use crate::util::ser::{Ser, SerAccept};
use crate::util::status_json::StatusJson as SJ;
use diesel::prelude::*;
use rocket::serde::json::Json;
use rocket::{State, get, post};
use std::collections::HashMap;
use strecklistan_api::book_account::{
    BookAccount, BookAccountId, BookAccountType, MasterAccounts, NewBookAccount,
};

#[get("/book_accounts")]
pub fn get_accounts(
    db_pool: &State<DatabasePool>,
    accept: SerAccept,
) -> Result<Ser<HashMap<BookAccountId, BookAccount>>, SJ> {
    let mut connection = db_pool.inner().get()?;

    let (transactions, accounts) = connection
        .transaction::<(Vec<Transaction>, Vec<relational::BookAccount>), SJ, _>(|connection| {
            use crate::schema::tables::book_accounts::dsl::book_accounts;
            use crate::schema::tables::transactions::dsl::{deleted_at, transactions};
            Ok((
                transactions.filter(deleted_at.is_null()).load(connection)?,
                book_accounts.load(connection)?,
            ))
        })?;

    let mut accounts: HashMap<_, BookAccount> = accounts
        .into_iter()
        .map(|acc| (acc.id, acc.into()))
        .collect();

    for tr in transactions.iter() {
        if let Some(account) = accounts.get_mut(&tr.credited_account) {
            account.credit(tr.amount.into());
        }

        if let Some(account) = accounts.get_mut(&tr.debited_account) {
            account.debit(tr.amount.into());
        }
    }

    Ok(accept.ser(accounts))
}

#[post("/book_account", data = "<account>")]
pub fn add_account(
    db_pool: &State<DatabasePool>,
    accept: SerAccept,
    account: Json<NewBookAccount>,
) -> Result<Ser<i32>, SJ> {
    let mut connection = db_pool.inner().get()?;

    use crate::schema::tables::book_accounts::dsl::*;

    Ok(accept.ser(
        diesel::insert_into(book_accounts)
            .values((
                name.eq(&account.name),
                account_type.eq(&account.account_type),
                creditor.eq(&account.creditor),
            ))
            .returning(id)
            .get_result(&mut connection)?,
    ))
}

#[get("/book_accounts/masters")]
pub fn get_master_accounts(
    db_pool: &State<DatabasePool>,
    accept: SerAccept,
) -> Result<Ser<MasterAccounts>, SJ> {
    let mut connection = db_pool.inner().get()?;
    use crate::schema::tables::book_accounts::dsl::*;

    // TODO: Get the values for the master accounts from some configuration.
    let bank_account_name = "Bankkonto";
    let cash_account_name = "Kontantkassa";
    let sales_account_name = "Försäljning";
    let purchases_account_name = "Inköp";

    connection.transaction::<_, SJ, _>(|connection| {
        // Make sure the accounts exist in the database
        diesel::insert_into(book_accounts)
            .values((
                name.eq(bank_account_name),
                account_type.eq(BookAccountType::Assets),
            ))
            .on_conflict_do_nothing()
            .execute(connection)?;
        diesel::insert_into(book_accounts)
            .values((
                name.eq(cash_account_name),
                account_type.eq(BookAccountType::Assets),
            ))
            .on_conflict_do_nothing()
            .execute(connection)?;
        diesel::insert_into(book_accounts)
            .values((
                name.eq(sales_account_name),
                account_type.eq(BookAccountType::Revenue),
            ))
            .on_conflict_do_nothing()
            .execute(connection)?;
        diesel::insert_into(book_accounts)
            .values((
                name.eq(purchases_account_name),
                account_type.eq(BookAccountType::Expenses),
            ))
            .on_conflict_do_nothing()
            .execute(connection)?;

        Ok(accept.ser(MasterAccounts {
            bank_account_id: book_accounts
                .filter(name.eq(bank_account_name))
                .select(id)
                .get_result(connection)?,
            cash_account_id: book_accounts
                .filter(name.eq(cash_account_name))
                .select(id)
                .get_result(connection)?,
            sales_account_id: book_accounts
                .filter(name.eq(sales_account_name))
                .select(id)
                .get_result(connection)?,
            purchases_account_id: book_accounts
                .filter(name.eq(purchases_account_name))
                .select(id)
                .get_result(connection)?,
        }))
    })
}
