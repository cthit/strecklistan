use crate::app::Msg;
use crate::generated::css_classes::C;
use crate::util::simple_ev;
use seed::app::cmds::timeout;
use seed::prelude::*;
use seed::*;
use std::collections::BTreeMap;

pub type NotificationId = u32;

#[derive(Debug, Clone)]
pub enum NotificationMessage {
    ShowNotification {
        duration_ms: u32,
        notification: Notification,
    },
    RemoveNotification(NotificationId),
}

#[derive(Default)]
pub struct NotificationManager {
    next_id: NotificationId,
    notifications: BTreeMap<NotificationId, Notification>,
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub title: String,
    pub body: Option<String>,
    pub notification_type: NotificationType,
}

#[derive(Debug, Clone)]
pub enum NotificationType {
    Error,
    Success,
}

impl NotificationManager {
    pub fn view(&self) -> Node<Msg> {
        div![
            C![C.notification_list],
            self.notifications.iter().map(|(id, notification)| {
                div![
                    C![C.notification],
                    match notification.notification_type {
                        NotificationType::Error => C![C.notification_error],
                        NotificationType::Success => C![C.notification_success],
                    },
                    p![C![C.notification_title], &notification.title],
                    if let Some(body) = &notification.body {
                        p![C![C.notification_body], &body]
                    } else {
                        empty![]
                    },
                    simple_ev(
                        Ev::Click,
                        Msg::Notification(NotificationMessage::RemoveNotification(*id))
                    ),
                ]
            })
        ]
    }

    pub fn update(&mut self, msg: NotificationMessage, orders: &mut impl Orders<Msg>) {
        match msg {
            NotificationMessage::ShowNotification {
                duration_ms,
                notification,
            } => {
                let id = self.next_id;
                self.next_id += 1;

                self.notifications.insert(id, notification);
                orders.perform_cmd(timeout(duration_ms, move || {
                    Msg::Notification(NotificationMessage::RemoveNotification(id))
                }));
            }
            NotificationMessage::RemoveNotification(id) => {
                self.notifications.remove(&id);
            }
        }
    }
}
