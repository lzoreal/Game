import QtQuick

Canvas {
    id: icon

    property string name: "search"
    property color color: "#ffffff"
    property real size: 18
    property real strokeWidth: 1.7

    width: size
    height: size

    onNameChanged: requestPaint()
    onColorChanged: requestPaint()
    onSizeChanged: requestPaint()

    onPaint: {
        var ctx = getContext("2d")
        ctx.resetTransform()
        ctx.clearRect(0, 0, width, height)
        var scale = size / 24
        ctx.scale(scale, scale)
        ctx.strokeStyle = color
        ctx.fillStyle = color
        ctx.lineWidth = strokeWidth
        ctx.lineCap = "round"
        ctx.lineJoin = "round"

        if (name === "search") {
            ctx.beginPath()
            ctx.arc(11, 11, 6.4, 0, Math.PI * 2)
            ctx.moveTo(15.8, 15.8)
            ctx.lineTo(20, 20)
            ctx.stroke()
        } else if (name === "plus") {
            ctx.beginPath()
            ctx.moveTo(12, 5); ctx.lineTo(12, 19)
            ctx.moveTo(5, 12); ctx.lineTo(19, 12)
            ctx.stroke()
        } else if (name === "folder") {
            ctx.beginPath()
            ctx.moveTo(3.8, 7.5); ctx.lineTo(9.5, 7.5); ctx.lineTo(11.3, 9.5)
            ctx.lineTo(20, 9.5); ctx.lineTo(20, 18.5); ctx.lineTo(4, 18.5)
            ctx.closePath()
            ctx.stroke()
        } else if (name === "refresh") {
            ctx.beginPath()
            ctx.arc(12, 12, 7.4, -0.45, 4.35, false)
            ctx.stroke()
            ctx.beginPath()
            ctx.moveTo(19.5, 5); ctx.lineTo(19.5, 11); ctx.lineTo(13.5, 11)
            ctx.stroke()
        } else if (name === "sun") {
            ctx.beginPath()
            ctx.arc(12, 12, 4, 0, Math.PI * 2)
            ctx.stroke()
            for (var i = 0; i < 8; i++) {
                var a = i * Math.PI / 4
                ctx.beginPath()
                ctx.moveTo(12 + Math.cos(a) * 7.2, 12 + Math.sin(a) * 7.2)
                ctx.lineTo(12 + Math.cos(a) * 10, 12 + Math.sin(a) * 10)
                ctx.stroke()
            }
        } else if (name === "moon") {
            ctx.beginPath()
            ctx.moveTo(15.5, 3.8)
            ctx.bezierCurveTo(8.8, 5.2, 6.5, 14.5, 12.5, 20)
            ctx.bezierCurveTo(6.2, 19.5, 3.8, 12.5, 8.2, 6.4)
            ctx.bezierCurveTo(10.2, 4.2, 13.2, 3.6, 15.5, 3.8)
            ctx.stroke()
        } else if (name === "more") {
            ctx.beginPath()
            ctx.arc(6, 12, 1.15, 0, Math.PI * 2)
            ctx.arc(12, 12, 1.15, 0, Math.PI * 2)
            ctx.arc(18, 12, 1.15, 0, Math.PI * 2)
            ctx.fill()
        } else if (name === "close") {
            ctx.beginPath()
            ctx.moveTo(6, 6); ctx.lineTo(18, 18)
            ctx.moveTo(18, 6); ctx.lineTo(6, 18)
            ctx.stroke()
        } else if (name === "minimize") {
            ctx.beginPath()
            ctx.moveTo(6, 12); ctx.lineTo(18, 12)
            ctx.stroke()
        } else if (name === "maximize") {
            ctx.strokeRect(6, 6, 12, 12)
        } else if (name === "restore") {
            ctx.strokeRect(8, 5, 10, 10)
            ctx.strokeRect(5, 8, 10, 10)
        } else if (name === "chevron") {
            ctx.beginPath()
            ctx.moveTo(8, 10); ctx.lineTo(12, 14); ctx.lineTo(16, 10)
            ctx.stroke()
        } else if (name === "link") {
            ctx.beginPath()
            ctx.moveTo(10, 13)
            ctx.bezierCurveTo(12.5, 15.5, 16, 14, 17.5, 11)
            ctx.bezierCurveTo(19, 8.5, 17, 5.5, 14.5, 6.5)
            ctx.stroke()
            ctx.beginPath()
            ctx.moveTo(14, 11)
            ctx.bezierCurveTo(11.5, 8.5, 8, 10, 6.5, 13)
            ctx.bezierCurveTo(5, 15.5, 7, 18.5, 9.5, 17.5)
            ctx.stroke()
        } else if (name === "copy") {
            ctx.strokeRect(8, 8, 11, 11)
            ctx.beginPath()
            ctx.moveTo(16, 8); ctx.lineTo(16, 6)
            ctx.bezierCurveTo(16, 5, 15, 4, 14, 4)
            ctx.lineTo(6, 4)
            ctx.bezierCurveTo(5, 4, 4, 5, 4, 6)
            ctx.lineTo(4, 14)
            ctx.bezierCurveTo(4, 15, 5, 16, 6, 16)
            ctx.lineTo(8, 16)
            ctx.stroke()
        } else if (name === "download") {
            ctx.beginPath()
            ctx.moveTo(12, 4); ctx.lineTo(12, 14)
            ctx.moveTo(8, 10); ctx.lineTo(12, 14); ctx.lineTo(16, 10)
            ctx.moveTo(5, 19); ctx.lineTo(19, 19)
            ctx.stroke()
        } else if (name === "pulse") {
            ctx.beginPath()
            ctx.moveTo(3, 12); ctx.lineTo(7, 12); ctx.lineTo(9, 7)
            ctx.lineTo(13, 17); ctx.lineTo(15, 12); ctx.lineTo(21, 12)
            ctx.stroke()
        } else if (name === "grid") {
            ctx.strokeRect(4, 4, 6, 6)
            ctx.strokeRect(14, 4, 6, 6)
            ctx.strokeRect(4, 14, 6, 6)
            ctx.strokeRect(14, 14, 6, 6)
        } else if (name === "magnet") {
            ctx.beginPath()
            ctx.moveTo(6, 4); ctx.lineTo(6, 13)
            ctx.bezierCurveTo(6, 18, 9, 20, 12, 20)
            ctx.bezierCurveTo(15, 20, 18, 18, 18, 13)
            ctx.lineTo(18, 4)
            ctx.moveTo(6, 9); ctx.lineTo(10, 9)
            ctx.moveTo(14, 9); ctx.lineTo(18, 9)
            ctx.stroke()
        } else if (name === "external") {
            ctx.beginPath()
            ctx.moveTo(14, 5); ctx.lineTo(19, 5); ctx.lineTo(19, 10)
            ctx.moveTo(19, 5); ctx.lineTo(10, 14)
            ctx.moveTo(19, 14); ctx.lineTo(19, 18)
            ctx.bezierCurveTo(19, 19, 18, 20, 17, 20)
            ctx.lineTo(6, 20)
            ctx.bezierCurveTo(5, 20, 4, 19, 4, 18)
            ctx.lineTo(4, 7)
            ctx.bezierCurveTo(4, 6, 5, 5, 6, 5)
            ctx.lineTo(10, 5)
            ctx.stroke()
        } else if (name === "check") {
            ctx.beginPath()
            ctx.moveTo(5, 12.5); ctx.lineTo(9.2, 16.5); ctx.lineTo(19, 6.5)
            ctx.stroke()
        }
    }
}
