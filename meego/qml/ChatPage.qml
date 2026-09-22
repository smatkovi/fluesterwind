import QtQuick 1.1
import com.nokia.meego 1.0

Page {
    id: seite
    property string titel: ""
    property string jid: ""

    tools: ToolBarLayout {
        ToolIcon {
            platformIconId: "toolbar-back"
            onClicked: { Dienst.chatSchliessen(); pageStack.pop() }
        }
        Label {
            text: seite.titel
            elide: Text.ElideRight
            maximumLineCount: 1
            font.pixelSize: 24
            width: parent.width - 140
            anchors.verticalCenter: parent.verticalCenter
        }
    }

    Rectangle { anchors.fill: parent; color: "#000000" }

    ListView {
        id: verlauf
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: eingabe.top
        anchors.margins: 8
        clip: true
        model: Dienst.nachrichten
        spacing: 6

        // Nur ans Ende springen, wenn man auch unten steht -- sonst reisst
        // eine eintreffende Nachricht einen aus dem, was man gerade liest.
        onCountChanged: if (atYEnd || count <= 1) positionViewAtEnd()
        Component.onCompleted: positionViewAtEnd()

        // Breiteste zulaessige Blase. Einmal hier statt in jedem Eintrag.
        property real maxBlase: width * 0.82

        delegate: Item {
            width: verlauf.width
            height: blase.height + 4

            Rectangle {
                id: blase
                anchors.right: modelData.fromMe ? parent.right : undefined
                anchors.left: modelData.fromMe ? undefined : parent.left
                width: spalte.width + 24
                height: spalte.height + 16
                radius: 10
                color: modelData.fromMe ? "#1f4d2e" : "#1c1c1c"

                // Der Messtext haengt an nichts und wird von nichts gelesen
                // ausser seiner eigenen Breite. Bestimmte die Blase ihre
                // Breite aus paintedWidth des umbrechenden Textes, kam
                // dessen Breite wiederum von der Blase -- diese Schleife
                // liess im WhatsApp-Port den ganzen Verlauf leer.
                Text {
                    id: messer
                    visible: false
                    text: inhalt.text
                    font.pixelSize: inhalt.font.pixelSize
                }

                Column {
                    id: spalte
                    x: 12
                    y: 8
                    width: Math.max(60, Math.min(verlauf.maxBlase - 24,
                                                 messer.paintedWidth))
                    spacing: 4

                    // In Gruppen ist ohne Absender nicht zu erkennen, wer
                    // spricht; im Einzelchat waere es nur Laerm.
                    Label {
                        width: parent.width
                        visible: !modelData.fromMe
                                 && Dienst.istGruppe(seite.jid)
                        height: visible ? implicitHeight : 0
                        text: modelData.sender || ""
                        color: "#6aa6d6"
                        font.pixelSize: 17
                        font.bold: true
                        elide: Text.ElideRight
                        maximumLineCount: 1
                    }

                    Label {
                        id: inhalt
                        width: parent.width
                        text: modelData.text || ""
                        wrapMode: Text.Wrap
                        font.pixelSize: 22
                    }

                    Label {
                        width: parent.width
                        horizontalAlignment: Text.AlignRight
                        text: Dienst.zeit(modelData.timestamp)
                        color: "#808080"
                        font.pixelSize: 15
                    }
                }
            }
        }
    }

    Rectangle {
        id: eingabe
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        height: feld.height + 16
        color: "#000000"

        // Die Trennlinie setzt die Eingabe vom Verlauf ab; ohne sie
        // schwimmt das Feld im Schwarz.
        Rectangle {
            anchors.top: parent.top
            anchors.left: parent.left
            anchors.right: parent.right
            height: 1
            color: "#2a2a2a"
        }

        Row {
            anchors.centerIn: parent
            width: parent.width - 16
            spacing: 8

            TextArea {
                id: feld
                width: parent.width - knopf.width - 8
                placeholderText: "Nachricht"
                // Enter schickt ab, statt eine Zeile einzufuegen: auf der
                // ausziehbaren Tastatur ist das die erwartete Geste.
                Keys.onReturnPressed: knopf.abschicken()
                Keys.onEnterPressed: knopf.abschicken()
            }

            Button {
                id: knopf
                width: 120
                text: "Senden"
                enabled: feld.text.trim() !== ""
                onClicked: abschicken()

                function abschicken() {
                    if (feld.text.trim() === "")
                        return
                    Dienst.senden(seite.jid, feld.text)
                    feld.text = ""
                }
            }
        }
    }

    Label {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: eingabe.top
        anchors.bottomMargin: 8
        visible: Dienst.fehler !== ""
        text: Dienst.fehler
        color: "#ff6060"
        font.pixelSize: 18
    }
}
