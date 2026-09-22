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
        // Der Name ist bei Gruppen zugleich der Griff zur
        // Mitgliederliste. Ein Symbol in der Leiste findet man erst, wenn
        // man danach sucht; auf den Namen zu tippen ist das, was man
        // ohnehin versucht.
        Item {
            width: parent.width - 150
            height: parent.height
            Label {
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width
                text: seite.titel
                elide: Text.ElideRight
                maximumLineCount: 1
                font.pixelSize: 24
                color: mitgliederBereich.pressed ? "#7fbf7f" : "#ffffff"
            }
            MouseArea {
                id: mitgliederBereich
                anchors.fill: parent
                enabled: Dienst.istGruppe(seite.jid)
                onClicked: pageStack.push(
                    Qt.resolvedUrl("GroupMembersPage.qml"),
                    { titel: seite.titel, jid: seite.jid })
            }
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

                // "image", "video", "audio", "document" -- leer ohne Anhang.
                property bool hatAnhang: modelData.mediaType !== undefined
                                         && modelData.mediaType !== ""
                property bool istBild: modelData.mediaType === "image"
                property bool geladen: modelData.localPath !== undefined
                                       && modelData.localPath !== ""

                Column {
                    id: spalte
                    x: 12
                    y: 8
                    // Mit Anhang lohnt die Rechnerei nach Textbreite nicht:
                    // Bild und Dateizeile wollen ohnehin die volle Breite.
                    width: blase.hatAnhang
                           ? verlauf.maxBlase - 24
                           : Math.max(60, Math.min(verlauf.maxBlase - 24,
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

                    // --- Bild -------------------------------------
                    Item {
                        width: parent.width
                        visible: blase.istBild
                        height: visible ? (blase.geladen ? bild.height : 96) : 0

                        Image {
                            id: bild
                            width: parent.width
                            fillMode: Image.PreserveAspectFit
                            // sourceSize kommt aus der Datei und haengt
                            // nicht an width -- keine Bindungsschleife.
                            height: (status === Image.Ready && sourceSize.width > 0)
                                    ? width * sourceSize.height / sourceSize.width
                                    : 0
                            source: blase.geladen ? "file://" + modelData.localPath : ""
                            asynchronous: true
                            smooth: true
                            MouseArea {
                                anchors.fill: parent
                                enabled: blase.geladen
                                onClicked: Dienst.oeffnen(modelData.localPath)
                            }
                        }
                        Button {
                            anchors.centerIn: parent
                            visible: !blase.geladen
                            width: Math.min(parent.width, 240)
                            text: "Bild laden"
                            onClicked: Dienst.medienLaden(seite.jid, modelData.id)
                        }
                    }

                    // --- Datei, Ton, Video ------------------------------
                    Item {
                        width: parent.width
                        visible: blase.hatAnhang && !blase.istBild
                        height: visible ? 64 : 0

                        Rectangle {
                            anchors.fill: parent
                            color: "#00000000"
                            border.color: "#3a3a3a"
                            border.width: 1
                            radius: 6
                        }
                        Column {
                            anchors.left: parent.left
                            anchors.leftMargin: 10
                            anchors.right: parent.right
                            anchors.rightMargin: 10
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 2
                            Label {
                                width: parent.width
                                text: modelData.fileName
                                      || ("Anhang (" + modelData.mediaType + ")")
                                elide: Text.ElideMiddle
                                maximumLineCount: 1
                                font.pixelSize: 19
                            }
                            Label {
                                width: parent.width
                                text: blase.geladen
                                      ? "Antippen zum Öffnen"
                                      : ("Antippen zum Laden · "
                                         + Dienst.groesse(modelData.size))
                                color: "#909090"
                                font.pixelSize: 16
                            }
                        }
                        MouseArea {
                            anchors.fill: parent
                            onClicked: blase.geladen
                                       ? Dienst.oeffnen(modelData.localPath)
                                       : Dienst.medienLaden(seite.jid, modelData.id)
                        }
                    }

                    Label {
                        id: inhalt
                        width: parent.width
                        visible: text !== ""
                        height: visible ? implicitHeight : 0
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
            spacing: 6

            ToolIcon {
                id: klammer
                anchors.verticalCenter: parent.verticalCenter
                platformIconId: "toolbar-attachment"
                onClicked: pageStack.push(Qt.resolvedUrl("FilesPage.qml"),
                                          { jid: seite.jid })
            }

            // TextField, nicht TextArea: dessen onAccepted feuert beim
            // Enter, waehrend eine TextArea die Taste selbst verbraucht
            // und eine Zeile einfuegt. Mit Keys.onReturnPressed kam die
            // Nachricht nicht heraus -- der WhatsApp-Port macht es
            // ebenfalls so, und dort geht es.
            TextField {
                id: feld
                width: parent.width - knopf.width - klammer.width - 12
                placeholderText: "Nachricht"
                onAccepted: knopf.abschicken()
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
