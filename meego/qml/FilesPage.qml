import QtQuick 1.1
import com.nokia.meego 1.0

// Der Dateiwaehler.
//
// Harmattan bringt keinen mit, den eine fremde App aufrufen koennte, also
// listet der Dienst und diese Seite zeigt. Die Liste ist bei 400
// Eintraegen gedeckelt: auf VFAT kostet jedes stat, und beim WhatsApp-Port
// fror das Geraet beim Oeffnen eines vollen Ordners ein.
Page {
    id: seite
    property string jid: ""

    tools: ToolBarLayout {
        ToolIcon {
            platformIconId: "toolbar-back"
            onClicked: pageStack.pop()
        }
        ToolIcon {
            platformIconId: "toolbar-up"
            visible: Dienst.verzeichnisEltern !== ""
            onClicked: Dienst.verzeichnisLesen(Dienst.verzeichnisEltern)
        }
        Label {
            text: "Datei wählen"
            font.pixelSize: 24
            anchors.verticalCenter: parent.verticalCenter
        }
    }

    Rectangle { anchors.fill: parent; color: "#000000" }

    Label {
        id: pfadZeile
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.margins: 8
        text: Dienst.verzeichnisPfad
        elide: Text.ElideLeft
        maximumLineCount: 1
        color: "#808080"
        font.pixelSize: 17
    }

    ListView {
        id: liste
        anchors.top: pfadZeile.bottom
        anchors.topMargin: 6
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        clip: true
        model: Dienst.verzeichnis
        cacheBuffer: 400

        delegate: Item {
            width: liste.width
            height: 68

            Label {
                id: zeichen
                x: 12
                anchors.verticalCenter: parent.verticalCenter
                width: 40
                text: modelData.isDir ? "▸" : "·"
                color: modelData.isDir ? "#6aa6d6" : "#707070"
                font.pixelSize: 26
            }
            Column {
                anchors.left: zeichen.right
                anchors.right: parent.right
                anchors.rightMargin: 12
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Label {
                    width: parent.width
                    text: modelData.name
                    elide: Text.ElideMiddle
                    maximumLineCount: 1
                    font.pixelSize: 22
                }
                Label {
                    width: parent.width
                    visible: !modelData.isDir
                    height: visible ? implicitHeight : 0
                    text: Dienst.groesse(modelData.size)
                    color: "#808080"
                    font.pixelSize: 16
                }
            }
            Rectangle {
                anchors.bottom: parent.bottom
                anchors.left: parent.left
                anchors.right: parent.right
                height: 1
                color: "#1a1a1a"
            }
            MouseArea {
                anchors.fill: parent
                onClicked: {
                    if (modelData.isDir) {
                        Dienst.verzeichnisLesen(modelData.path)
                    } else {
                        Dienst.anhangSenden(seite.jid, modelData.path, "")
                        pageStack.pop()
                    }
                }
            }
        }
    }

    Label {
        anchors.centerIn: parent
        visible: liste.count === 0
        color: "#707070"
        text: "Leer"
    }

    Component.onCompleted: Dienst.verzeichnisLesen("")
}
