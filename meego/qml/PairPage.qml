import QtQuick 1.1
import com.nokia.meego 1.0

// Das Geraet als Zweitgeraet anmelden.
//
// Signal kennt keinen Ziffern-Code wie WhatsApp -- es gibt nur den Weg
// ueber ein Hauptgeraet, das ein Zweitgeraet aufnimmt. Das N950 kann
// keinen Code scannen, aber einen zeigen: der Dienst malt ihn, diese
// Seite zeigt ihn an, und das Hauptgeraet schaut hin.
Page {
    id: seite

    Rectangle { anchors.fill: parent; color: "#000000" }

    Flickable {
        anchors.fill: parent
        anchors.margins: 16
        contentHeight: inhalt.height

        Column {
            id: inhalt
            width: parent.width
            spacing: 16

            Item { width: 1; height: 8 }

            Label {
                width: parent.width
                text: "Mit Signal verknüpfen"
                font.pixelSize: 32
                font.bold: true
            }

            Label {
                width: parent.width
                wrapMode: Text.WordWrap
                color: "#b0b0b0"
                text: Dienst.kopplungsAdresse !== ""
                      ? "Am Hauptgerät die Liste der verknüpften Geräte "
                        + "öffnen und diesen Code abscannen. Er gilt nur "
                        + "wenige Minuten."
                      : "Dieses Gerät meldet sich als Zweitgerät an. Dein "
                        + "Hauptgerät muss dabei erreichbar sein."
            }

            Button {
                width: parent.width
                text: Dienst.kopplungsAdresse !== ""
                      ? "Neuen Code anfordern" : "Code erzeugen"
                enabled: Dienst.zustand !== "kein Dienst"
                onClicked: Dienst.koppeln()
            }

            // Der Code selbst. Weiss hinterlegt, weil manche Scanner an
            // einem dunkel umrandeten Code scheitern -- die Ruhezone
            // gehoert hell.
            Rectangle {
                width: parent.width
                height: width
                visible: Dienst.kopplungsBild !== ""
                color: "#ffffff"
                radius: 4

                Image {
                    anchors.centerIn: parent
                    width: Math.min(parent.width - 16, parent.height - 16)
                    height: width
                    source: Dienst.kopplungsBild
                    // Ein QR-Code lebt von harten Kanten; Glaettung
                    // verwischt die Module und kostet Lesbarkeit.
                    smooth: false
                    fillMode: Image.PreserveAspectFit
                    asynchronous: true
                    cache: false
                }
            }

            Label {
                width: parent.width
                visible: Dienst.kopplungsAdresse !== ""
                wrapMode: Text.WrapAnywhere
                color: "#707070"
                font.pixelSize: 15
                text: Dienst.kopplungsAdresse
            }

            Label {
                width: parent.width
                wrapMode: Text.WordWrap
                color: "#ff6060"
                visible: Dienst.fehler !== ""
                text: Dienst.fehler
            }

            Label {
                width: parent.width
                color: "#808080"
                font.pixelSize: 18
                text: "Zustand: " + Dienst.zustand
            }
        }
    }
}
