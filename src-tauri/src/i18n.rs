use std::sync::Mutex;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum AppLocale {
    #[default]
    ZhCn,
    EnUs,
}

impl AppLocale {
    pub(crate) fn from_code(value: &str) -> Result<Self, String> {
        match value {
            "zh-CN" => Ok(Self::ZhCn),
            "en-US" => Ok(Self::EnUs),
            _ => Err(format!("unsupported locale: {value}")),
        }
    }

    pub(crate) fn app_title(self) -> &'static str {
        match self {
            Self::ZhCn => "蛋定 Todo",
            Self::EnUs => "EggDone",
        }
    }

    pub(crate) fn focus_window_title(self) -> &'static str {
        match self {
            Self::ZhCn => "蛋定专注",
            Self::EnUs => "EggDone Focus",
        }
    }

    pub(crate) fn tray_toggle(self) -> &'static str {
        match self {
            Self::ZhCn => "打开 / 隐藏面板",
            Self::EnUs => "Show / hide panel",
        }
    }

    pub(crate) fn tray_new_task(self) -> &'static str {
        match self {
            Self::ZhCn => "新增任务",
            Self::EnUs => "New task",
        }
    }

    pub(crate) fn tray_today(self) -> &'static str {
        match self {
            Self::ZhCn => "今天任务",
            Self::EnUs => "Today's tasks",
        }
    }

    pub(crate) fn tray_focus_start(self) -> &'static str {
        match self {
            Self::ZhCn => "开始专注",
            Self::EnUs => "Start focus",
        }
    }

    pub(crate) fn tray_focus_toggle(self) -> &'static str {
        match self {
            Self::ZhCn => "暂停 / 继续专注",
            Self::EnUs => "Pause / resume focus",
        }
    }

    pub(crate) fn tray_focus_end(self) -> &'static str {
        match self {
            Self::ZhCn => "结束专注",
            Self::EnUs => "End focus",
        }
    }

    pub(crate) fn tray_about(self) -> &'static str {
        match self {
            Self::ZhCn => "关于 EggDone",
            Self::EnUs => "About EggDone",
        }
    }

    pub(crate) fn tray_quit(self) -> &'static str {
        match self {
            Self::ZhCn => "退出",
            Self::EnUs => "Quit",
        }
    }

    pub(crate) fn focus_phase(self, phase: &str) -> &'static str {
        match (self, phase) {
            (Self::ZhCn, "break") => "休息",
            (Self::ZhCn, _) => "专注",
            (Self::EnUs, "break") => "Break",
            (Self::EnUs, _) => "Focus",
        }
    }

    pub(crate) fn task_tooltip(self, remaining: u32, total: u32, today_due: u32) -> String {
        match self {
            Self::ZhCn => {
                format!("蛋定 Todo · {remaining}/{total} 项未完成 · 今天 {today_due} 项")
            }
            Self::EnUs => {
                format!("EggDone · {remaining}/{total} incomplete · {today_due} today")
            }
        }
    }

    pub(crate) fn reminder_body(self, title: &str) -> String {
        match self {
            Self::ZhCn => format!("该处理了：{title}"),
            Self::EnUs => format!("Time to work on: {title}"),
        }
    }

    pub(crate) fn reminder_snooze(self) -> &'static str {
        match self {
            Self::ZhCn => "稍后 10 分钟",
            Self::EnUs => "In 10 minutes",
        }
    }

    pub(crate) fn reminder_later_today(self) -> &'static str {
        match self {
            Self::ZhCn => "今天晚些时候",
            Self::EnUs => "Later today",
        }
    }

    pub(crate) fn focus_notification_title(self) -> &'static str {
        match self {
            Self::ZhCn => "蛋定专注",
            Self::EnUs => "EggDone Focus",
        }
    }

    pub(crate) fn focus_notification_body(self, completed_phase: &str) -> &'static str {
        match (self, completed_phase) {
            (Self::ZhCn, "break") => "休息结束，可以开始下一轮专注。",
            (Self::ZhCn, _) => "专注结束，先休息一下。",
            (Self::EnUs, "break") => "Break finished. Ready for the next focus session.",
            (Self::EnUs, _) => "Focus session finished. Take a short break.",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FocusTooltipSnapshot {
    pub(crate) phase: String,
    pub(crate) remaining_ms: u64,
    pub(crate) title: Option<String>,
}

#[derive(Default)]
struct I18nStateInner {
    locale: AppLocale,
    focus_tooltip: Option<FocusTooltipSnapshot>,
}

#[derive(Default)]
pub(crate) struct I18nState {
    inner: Mutex<I18nStateInner>,
}

impl I18nState {
    pub(crate) fn locale(&self) -> AppLocale {
        self.inner
            .lock()
            .map(|inner| inner.locale)
            .unwrap_or_default()
    }

    pub(crate) fn set_locale(&self, locale: AppLocale) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.locale = locale;
        }
    }

    pub(crate) fn set_focus_tooltip(&self, snapshot: FocusTooltipSnapshot) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.focus_tooltip = Some(snapshot);
        }
    }

    pub(crate) fn clear_focus_tooltip(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.focus_tooltip = None;
        }
    }

    pub(crate) fn focus_tooltip(&self) -> Option<FocusTooltipSnapshot> {
        self.inner
            .lock()
            .ok()
            .and_then(|inner| inner.focus_tooltip.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_supported_resolved_locales() {
        assert_eq!(AppLocale::from_code("zh-CN"), Ok(AppLocale::ZhCn));
        assert_eq!(AppLocale::from_code("en-US"), Ok(AppLocale::EnUs));
        assert!(AppLocale::from_code("system").is_err());
    }

    #[test]
    fn keeps_user_content_unchanged_in_dynamic_messages() {
        assert_eq!(
            AppLocale::EnUs.reminder_body("写周报"),
            "Time to work on: 写周报"
        );
    }
}
