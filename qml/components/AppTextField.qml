import QtQuick
import QtQuick.Controls

TextField {
    id: control

    implicitHeight: 38
    leftPadding: 12
    rightPadding: 12
    selectByMouse: true
    color: theme.colors.text
    placeholderTextColor: theme.colors.placeholder
    font.pixelSize: 12
    selectionColor: theme.colors.accent_soft
    selectedTextColor: theme.colors.text

    background: Rectangle {
        color: theme.colors.soft
        radius: 9
        border.width: 1
        border.color: control.activeFocus ? theme.colors.accent : theme.colors.border
        Behavior on border.color { ColorAnimation { duration: 110 } }
    }
}
