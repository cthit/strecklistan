use crate::generated::css_classes::C;
use crate::util::simple_ev;
use seed::prelude::*;
use seed::*;
use strecklistan_api::book_account::BookAccount;
use strecklistan_api::member::Member;

pub fn view_tillgodo<M: 'static + Clone>(
    account: &BookAccount,
    member: &Member,
    msg: M,
) -> Node<M> {
    let tillgodo_money_class = if account.balance < 0.into() {
        C![C.tillgodo_money, C.tillgodo_money_angry]
    } else {
        C![C.tillgodo_money]
    };

    div![
        C![C.tillgodo_entry],
        div![
            C![C.tillgodo_nick],
            member.nickname.as_deref().unwrap_or(""),
        ],
        div![
            C![C.tillgodo_name],
            member.first_name.clone(),
            " ",
            member.last_name.clone(),
        ],
        div![tillgodo_money_class, format!("{}:-", account.balance)],
        simple_ev(Ev::Click, msg),
    ]
}
