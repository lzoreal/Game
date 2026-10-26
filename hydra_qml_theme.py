"""Theme tokens and a QObject bridge for the QML UI."""

from PySide6.QtCore import QObject, Property, QSettings, Signal

SETTINGS_ORG = "GameMagNet"
SETTINGS_APP = "GameMagNet"
LIGHT_THEME_KEY = "appearance/light"

DARK_THEME = {
    "window": "#0c1118",
    "topbar": "#0c1118",
    "sidebar": "#0a0e14",
    "surface": "#131a23",
    "soft": "#0f151d",
    "hover": "#1a232e",
    "border": "#222b37",
    "border_strong": "#344152",
    "text": "#e9eef7",
    "muted": "#95a2b5",
    "faint": "#687588",
    "accent": "#7d93ff",
    "accent_hover": "#93a6ff",
    "accent_soft": "#18213a",
    "accent_text": "#c4cfff",
    "success": "#4ade9b",
    "success_soft": "#10261f",
    "warning": "#f5b56a",
    "button_bg": "#131a23",
    "button_text": "#e9eef7",
    "disabled_bg": "#0f151d",
    "disabled_border": "#222b37",
    "tag_bg": "#0f151d",
    "tag_text": "#95a2b5",
    "chip_bg": "#18213a",
    "chip_text": "#c4cfff",
    "link": "#7d93ff",
    "link_hover": "#93a6ff",
    "desc_bg": "#0f151d",
    "desc_border": "#222b37",
    "desc_text": "#aeb9c9",
    "table_alt": "#0f151d",
    "table_selection": "#18213a",
    "table_selection_text": "#e9eef7",
    "header_text": "#687588",
    "header_line": "#222b37",
    "menu_selection": "#18213a",
    "source_text": "#95a2b5",
    "source_hover": "#1a232e",
    "source_selected": "#18213a",
    "source_selected_text": "#c4cfff",
    "scroll": "#344152",
    "scroll_hover": "#95a2b5",
    "progress_bg": "#0f151d",
    "splitter": "#222b37",
    "tooltip_bg": "#131a23",
    "tooltip_border": "#344152",
    "tooltip_text": "#e9eef7",
    "titlebar": "#0c1118",
    "titlebar_text": "#e9eef7",
    "empty_icon": "#687588",
    "empty_title": "#e9eef7",
    "section": "#95a2b5",
    "label": "#e9eef7",
    "placeholder": "#687588",
}

LIGHT_THEME = {
    "window": "#f6f8fc",
    "topbar": "#f6f8fc",
    "sidebar": "#eef2f8",
    "surface": "#ffffff",
    "soft": "#f6f8fc",
    "hover": "#eef3fa",
    "border": "#e2e7f0",
    "border_strong": "#cfd8e5",
    "text": "#131a26",
    "muted": "#667085",
    "faint": "#98a2b3",
    "accent": "#4f6bff",
    "accent_hover": "#3f59ed",
    "accent_soft": "#edf0ff",
    "accent_text": "#394dcc",
    "success": "#0f7a5a",
    "success_soft": "#e8f7f1",
    "warning": "#b54708",
    "button_bg": "#ffffff",
    "button_text": "#131a26",
    "disabled_bg": "#f6f8fc",
    "disabled_border": "#e2e7f0",
    "tag_bg": "#f6f8fc",
    "tag_text": "#667085",
    "chip_bg": "#edf0ff",
    "chip_text": "#394dcc",
    "link": "#4f6bff",
    "link_hover": "#3f59ed",
    "desc_bg": "#f6f8fc",
    "desc_border": "#e2e7f0",
    "desc_text": "#4b5563",
    "table_alt": "#f6f8fc",
    "table_selection": "#edf0ff",
    "table_selection_text": "#131a26",
    "header_text": "#98a2b3",
    "header_line": "#e2e7f0",
    "menu_selection": "#edf0ff",
    "source_text": "#667085",
    "source_hover": "#eef3fa",
    "source_selected": "#edf0ff",
    "source_selected_text": "#394dcc",
    "scroll": "#cfd8e5",
    "scroll_hover": "#98a2b3",
    "progress_bg": "#f6f8fc",
    "splitter": "#e2e7f0",
    "tooltip_bg": "#ffffff",
    "tooltip_border": "#cfd8e5",
    "tooltip_text": "#131a26",
    "titlebar": "#f6f8fc",
    "titlebar_text": "#131a26",
    "empty_icon": "#c3cbd8",
    "empty_title": "#131a26",
    "section": "#667085",
    "label": "#131a26",
    "placeholder": "#98a2b3",
}


class ThemeBridge(QObject):
    colorsChanged = Signal()
    darkChanged = Signal()

    def __init__(self, parent=None):
        super().__init__(parent)
        settings = QSettings(SETTINGS_ORG, SETTINGS_APP)
        self._dark = not bool(
            settings.value(LIGHT_THEME_KEY, False, type=bool)
        )

    @Property(bool, notify=darkChanged)
    def dark(self):
        return self._dark

    @dark.setter
    def dark(self, value):
        value = bool(value)
        if value == self._dark:
            return
        self._dark = value
        settings = QSettings(SETTINGS_ORG, SETTINGS_APP)
        settings.setValue(LIGHT_THEME_KEY, not value)
        self.darkChanged.emit()
        self.colorsChanged.emit()

    @Property("QVariantMap", notify=colorsChanged)
    def colors(self):
        return dict(DARK_THEME if self._dark else LIGHT_THEME)
