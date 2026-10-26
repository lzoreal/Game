import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Effects

Button {
    id: control

    property string kind: "soft"
    property string iconName: ""
    property int iconSize: 16
    property bool danger: false
    property int textSize: 12
    property int hPadding: 12
    property int contentSpacing: 6
    property int cornerRadius: 9
    property bool iconRight: false

    implicitHeight: 34
    implicitWidth: text.length > 0 ? contentRow.implicitWidth + hPadding * 2 : 34
    padding: 0
    hoverEnabled: true

    contentItem: Item {
        implicitWidth: contentRow.implicitWidth
        implicitHeight: contentRow.implicitHeight

        RowLayout {
            id: contentRow
            anchors.centerIn: parent
            spacing: control.text.length > 0 && control.iconName.length > 0 ? control.contentSpacing : 0

            AppIcon {
                visible: control.iconName.length > 0 && !control.iconRight
                Layout.alignment: Qt.AlignVCenter
                name: control.iconName
                size: control.iconSize
                color: control.iconColor
            }

            Text {
                visible: control.text.length > 0
                Layout.alignment: Qt.AlignVCenter
                text: control.text
                color: control.foregroundColor
                font.pixelSize: control.textSize
                font.weight: 500
                elide: Text.ElideRight
            }

            AppIcon {
                visible: control.iconName.length > 0 && control.iconRight
                Layout.alignment: Qt.AlignVCenter
                name: control.iconName
                size: control.iconSize
                color: control.iconColor
            }
        }
    }

    readonly property color iconColor: {
        if (kind === "primary" || kind === "danger")
            return "#ffffff"
        if (danger && hovered)
            return "#ffffff"
        if (kind === "filter" && checked)
            return theme.colors.accent_text
        if (kind === "soft")
            return theme.colors.text
        return theme.colors.muted
    }

    readonly property color foregroundColor: {
        if (kind === "primary" || kind === "danger")
            return "#ffffff"
        if (danger && hovered)
            return "#ffffff"
        if (kind === "filter" && checked)
            return theme.colors.accent_text
        if (kind === "ghost" || kind === "filter")
            return theme.colors.muted
        return theme.colors.text
    }

    background: Item {
        RectangularShadow {
            anchors.fill: surface
            radius: control.cornerRadius
            blur: 10
            color: theme.dark ? "#66000000" : "#2e000000"
            offset.x: 0
            offset.y: 2
            visible: control.checked || control.down
        }

        Rectangle {
            id: surface
            anchors.fill: parent
            radius: control.cornerRadius
            color: {
                if (kind === "danger")
                    return control.down ? "#c9383d" : (control.hovered ? "#d93b40" : "#e5484d")
                if (danger && control.hovered)
                    return "#e5484d"
                if (kind === "primary")
                    return control.down ? theme.colors.accent : (control.hovered ? theme.colors.accent_hover : theme.colors.accent)
                if (kind === "filter" && control.checked)
                    return theme.colors.accent_soft
                if (kind === "ghost" || kind === "filter")
                    return control.hovered ? theme.colors.hover : "transparent"
                return control.hovered ? theme.colors.hover : theme.colors.soft
            }
            border.width: kind === "ghost" || kind === "filter" || danger || kind === "danger" ? 0 : 1
            border.color: kind === "primary"
                          ? theme.colors.accent
                          : (kind === "filter" && control.checked ? "transparent" : theme.colors.border)
        }
    }
}
