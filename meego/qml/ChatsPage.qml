import QtQuick 1.1
import com.nokia.meego 1.0

Page {
    id: seite

    tools: ToolBarLayout {
        ToolIcon {
            platformIconId: "toolbar-refresh"
            onClicked: Dienst.neuLaden()
        }
        ToolIcon {
            platformIconId: "toolbar-view-menu"
            onClicked: menue.open()
        }
    }

    Menu {
        id: menue
        MenuLayout {
            MenuItem {
                text: "Zustand: " + Dienst.zustand
                      + (Dienst.nummer !== "" ? " · " + Dienst.nummer : "")
            }
            MenuItem {
                text: "Verknüpfte Geräte (" + Dienst.geraete.length + ")"
                onClicked: geraeteFenster.open()
            }
            MenuItem {
                text: "Abmelden"
                onClicked: abmeldeFrage.open()
            }
        }
    }

    // Was der Server zum Konto führt. Die eigene Sicht ("verknüpft")
    // sagt wenig -- erst diese Liste beweist, dass das Gerät angekommen
    // ist.
    QueryDialog {
        id: geraeteFenster
        titleText: "Verknüpfte Geräte"
        message: Dienst.geraete.length > 0
                 ? Dienst.geraete.join("\n")
                 : "Noch keine Auskunft vom Server."
        acceptButtonText: "Schließen"
    }

    QueryDialog {
        id: abmeldeFrage
        titleText: "Abmelden"
        // Ehrlich sagen, was dieser Knopf kann und was nicht: ein
        // Zweitgerät darf sich bei Signal nicht selbst vom Konto nehmen,
        // das verweigert der Server.
        message: "Anmeldedaten und lokale Datenbank werden gelöscht. "
                 + "Deine Nachrichten auf dem Hauptgerät bleiben unberührt.\n\n"
                 + "Aus der Liste der verknüpften Geräte entfernt sich dieses "
                 + "Gerät dabei nicht — das kann nur das Hauptgerät. Dort also "
                 + "noch „Fluesterwind (N9)" + String.fromCharCode(0x201C)
                 + " aus den verknüpften Geräten nehmen."
        acceptButtonText: "Abmelden"
        rejectButtonText: "Abbrechen"
        onAccepted: Dienst.abmelden()
    }

    Rectangle { anchors.fill: parent; color: "#000000" }

    ListView {
        id: liste
        anchors.fill: parent
        clip: true
        model: Dienst.chats
        cacheBuffer: 600

        delegate: Item {
            width: liste.width
            height: 88

            Rectangle {
                id: bildchen
                x: 12
                anchors.verticalCenter: parent.verticalCenter
                width: 64; height: 64; radius: 32
                color: Dienst.istGruppe(modelData.jid) ? "#3a5a3a" : "#2a4d6a"
                // Der Dienst legt geholte Bilder als Datei ab. Fehlt eines,
                // ist der Kreis mit dem Anfangsbuchstaben besser als ein
                // Loch -- und der Dienst holt sie nach und nach nach.
                Image {
                    anchors.fill: parent
                    source: modelData.avatar ? "file://" + modelData.avatar : ""
                    visible: status === Image.Ready
                    fillMode: Image.PreserveAspectCrop
                    smooth: true
                    asynchronous: true
                }
                Label {
                    anchors.centerIn: parent
                    visible: !modelData.avatar
                    text: (modelData.name || "?").substring(0, 1).toUpperCase()
                    font.pixelSize: 28
                }
            }

            Column {
                anchors.left: bildchen.right
                anchors.leftMargin: 12
                anchors.right: rechts.left
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2

                Label {
                    width: parent.width
                    text: modelData.name || modelData.jid
                    elide: Text.ElideRight
                    maximumLineCount: 1
                    font.pixelSize: 26
                }
                Label {
                    width: parent.width
                    // Echte Zeilenumbrueche in der Vorschau liessen die
                    // Zeilen uebereinander rutschen -- deshalb auf eine
                    // Zeile beschneiden und Leerraum einebnen.
                    text: (modelData.fromMe ? "Du: " : "")
                          + (modelData.lastMessage || "")
                              .replace(/\s+/g, " ")
                    elide: Text.ElideRight
                    maximumLineCount: 1
                    clip: true
                    color: "#909090"
                    font.pixelSize: 19
                }
            }

            Label {
                id: rechts
                anchors.right: parent.right
                anchors.rightMargin: 12
                anchors.verticalCenter: parent.verticalCenter
                text: Dienst.zeit(modelData.lastTime)
                color: "#707070"
                font.pixelSize: 17
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
                                   { titel: modelData.name || modelData.jid,
                                     jid: modelData.jid })
                }
            }
        }
    }

    Label {
        anchors.centerIn: parent
        visible: liste.count === 0
        color: "#707070"
        wrapMode: Text.WordWrap
        width: parent.width - 60
        horizontalAlignment: Text.AlignHCenter
        text: Dienst.verknuepft
              ? "Noch keine Chats. Signal überträgt an ein neu verknüpftes "
                + "Gerät keinen alten Verlauf — was ab jetzt kommt, steht hier."
              : "Nicht verknüpft"
    }
}
