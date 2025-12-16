use crate::components::select::{SelectInput, SelectInputMsg};
use crate::generated::css_classes::C;
use crate::util::{CompareToStr, simple_ev};
use seed::prelude::*;
use seed::*;
use std::cmp::Ordering;

#[derive(Clone, Debug)]
pub enum FilterMenuMsg {
    AddFilter,
    SetValue {
        filter_i: usize,
        value: String,
    },
    DeleteFilter {
        filter_i: usize,
    },

    FilterFieldMsg {
        filter_i: usize,
        msg: SelectInputMsg<usize>,
    },

    FilterOpMsg {
        filter_i: usize,
        msg: SelectInputMsg<FilterOp>,
    },
}

#[derive(Clone, Copy, Debug)]
pub enum FilterOp {
    Equals,
    NotEquals,
    GrTh,
    GrEq,
    LeTh,
    LeEq,
}

const ALL_OPS: &[FilterOp] = &[
    // first element is the default
    FilterOp::NotEquals,
    FilterOp::Equals,
    FilterOp::GrTh,
    FilterOp::GrEq,
    FilterOp::LeTh,
    FilterOp::LeEq,
];

#[derive(Clone)]
pub struct FilterMenu {
    /// The labels of fields that are being filtered on
    // NOTE: this is a boxed slice since because mutating it would break the field SelectInput on
    // the filters. They take a copy of this slice, which would also need to be updated.
    fields: Box<[&'static str]>,

    filters: Vec<FilterEntry>,
}

#[derive(Clone)]
struct FilterEntry {
    //field: usize,
    field: SelectInput<usize>,
    op: SelectInput<FilterOp>,
    value: String,
}

impl FilterMenu {
    pub fn new(fields: Vec<&'static str>) -> Self {
        assert_ne!(fields.len(), 0);
        FilterMenu {
            fields: fields.into_boxed_slice(),
            filters: vec![],
        }
    }

    pub fn filter(&self, values: &[&dyn CompareToStr]) -> bool {
        self.filters.iter().all(|filter| {
            // get the index of the selected field
            let selected_field = *filter.field.selected();

            // get the value that matches the selected field
            let value = &values[selected_field];

            // compare against the filter value
            let ord = value.cmp_to_str(&filter.value);

            matches!(
                (*filter.op.selected(), ord),
                (FilterOp::GrTh, Ordering::Greater)        // >  true if greater
                | (FilterOp::GrEq, Ordering::Greater)      // >=     ... greater
                | (FilterOp::GrEq, Ordering::Equal)        // >=     ... equals
                | (FilterOp::LeTh, Ordering::Less)         // <      ... less
                | (FilterOp::LeEq, Ordering::Less)         // <=     ... less
                | (FilterOp::LeEq, Ordering::Equal)        // <=     ... equals
                | (FilterOp::NotEquals, Ordering::Greater) // !=     ... greater
                | (FilterOp::NotEquals, Ordering::Less)    // !=     ... less
                | (FilterOp::Equals, Ordering::Equal) // ==     ... equals
            )
        })
    }

    pub fn update(&mut self, msg: FilterMenuMsg, orders: &mut impl Orders<FilterMenuMsg>) {
        match msg {
            FilterMenuMsg::AddFilter => {
                let fields = self.fields.clone();
                self.filters.push(FilterEntry {
                    //field: 0,
                    field: SelectInput::new(
                        self.fields.iter().enumerate().map(|(i, _)| i).collect(),
                        move |&i| fields[i],
                    )
                    .with_select_styles(&[C.filter_menu_item_elem, C.filter_menu_field]),
                    op: SelectInput::new(ALL_OPS.to_vec(), FilterOp::as_str)
                        .with_select_styles(&[C.filter_menu_item_elem, C.filter_menu_operator]),
                    value: String::new(),
                })
            }
            FilterMenuMsg::SetValue { filter_i, value } => self.filters[filter_i].value = value,
            FilterMenuMsg::DeleteFilter { filter_i } => {
                self.filters.remove(filter_i);
            }

            FilterMenuMsg::FilterFieldMsg { filter_i, msg } => self.filters[filter_i].field.update(
                msg,
                &mut orders.proxy(move |msg| FilterMenuMsg::FilterFieldMsg { filter_i, msg }),
            ),

            FilterMenuMsg::FilterOpMsg { filter_i, msg } => self.filters[filter_i].op.update(
                msg,
                &mut orders.proxy(move |msg| FilterMenuMsg::FilterOpMsg { filter_i, msg }),
            ),
        }
    }

    pub fn view(&self) -> Node<FilterMenuMsg> {
        div![
            button![
                C![C.wide_button],
                simple_ev(Ev::Click, FilterMenuMsg::AddFilter),
                "➕",
            ],
            div![
                self.filters
                    .iter()
                    .enumerate()
                    .map(|(filter_i, filter)| {
                        div![
                            C![C.filter_menu_item],
                            // show the filter field select tag
                            filter
                                .field
                                .view()
                                .map_msg(move |msg| FilterMenuMsg::FilterFieldMsg {
                                    msg,
                                    filter_i
                                }),
                            // show the filter operator select tag
                            filter
                                .op
                                .view()
                                .map_msg(move |msg| FilterMenuMsg::FilterOpMsg { msg, filter_i }),
                            // show the filter value input
                            input![
                                C![C.filter_menu_item_elem, C.filter_menu_value],
                                attrs! { At::Value => filter.value },
                                input_ev(Ev::Input, move |value| FilterMenuMsg::SetValue {
                                    filter_i,
                                    value,
                                }),
                            ],
                            button![
                                simple_ev(Ev::Click, FilterMenuMsg::DeleteFilter { filter_i }),
                                C![C.filter_menu_delete],
                                "✖",
                            ]
                        ]
                    })
                    .collect::<Vec<_>>(),
            ],
        ]
    }
}

impl FilterOp {
    pub fn as_str(&self) -> &'static str {
        match self {
            FilterOp::NotEquals => "!=",
            FilterOp::Equals => "==",
            FilterOp::GrTh => ">",
            FilterOp::GrEq => ">=",
            FilterOp::LeTh => "<",
            FilterOp::LeEq => "<=",
        }
    }
}
