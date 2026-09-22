import QtQuick 1.1
import com.nokia.meego 1.0

// Die Mitglieder einer Gruppe, mit dem Weg in den Einzelchat.
//
// Antippen oeffnet den Chat mit der Person -- auch mit jemandem, der in
// keiner Chatliste steht. Damit ist das zugleich der bequemste Weg,
// jemanden aus einer Gruppe zum ersten Mal anzuschreiben.
Page {
    id: seite
    property string titel: ""
    property string jid: ""

    tools: ToolBarLayout {
        ToolIcon {
            platformIconId: "toolbar-back"
            onClicked: pageStack.pop()
        }
        Label {
            text: Dienst.mitglieder.length + " Mitglieder"
            font.pixelSize: 24
            anchors.verticalCenter: parent.verticalCenter
        }
    }

    Rectangle { anchors.fill: parent; color: "#000000" }

    ListView {
        id: liste
        anchors.fill: parent
        anchors.margins: 4
        clip: true
        model: Dienst.mitglieder
        cacheBuffer: 400

        delegate: Item {
            width: liste.width
            height: 72

            Rectangle {
                id: bildchen
                x: 10
                anchors.verticalCenter: parent.verticalCenter
                width: 54; height: 54; radius: 27
                color: "#2a4d6a"
                Label {
                    anchors.centerIn: parent
                    text: (modelData.name || "?").substring(0, 1).toUpperCase()
                    font.pixelSize: 24
                }
            }
            Column {
                anchors.left: bildchen.right
                anchors.leftMargin: 12
                anchors.right: abzeichen.left
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Label {
                    width: parent.width
                    text: modelData.name
                    elide: Text.ElideRight
                    maximumLineCount: 1
                    font.pixelSize: 24
                }
            }
            Label {
                id: abzeichen
                anchors.right: parent.right
                anchors.rightMargin: 10
                anchors.verticalCenter: parent.verticalCenter
                visible: modelData.isAdmin === true
                text: "Admin"
                color: "#5fa85f"
                font.pixelSize: 18
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
                    Dienst.chatOeffnen(modelData.jid)
                    pageStack.push(Qt.resolvedUrl("ChatPage.qml"),
                                   { titel: modelData.name, jid: modelData.jid })
                }
            }
        }
    }

    Label {
        anchors.centerIn: parent
        color: "#707070"
        visible: Dienst.mitglieder.length === 0
        text: "Keine Mitglieder bekannt"
    }

    Component.onCompleted: Dienst.gruppeLaden(seite.jid)
}
