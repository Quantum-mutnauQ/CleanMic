use gettextrs::gettext;

pub fn tr(msgid: &str) -> String {
    gettext(msgid)
}
