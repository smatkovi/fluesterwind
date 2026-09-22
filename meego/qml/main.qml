import QtQuick 1.1
import com.nokia.meego 1.0

PageStackWindow {
    id: fenster
    showStatusBar: true
    showToolBar: true

    // Durchgaengig dunkel. Das N9 zeigt Schwarz auf seinem AMOLED wirklich
    // schwarz -- und im Dunkeln ist das der Unterschied zwischen lesbar
    // und blendend.
    platformStyle: PageStackWindowStyle { background: "" }
    Component.onCompleted: theme.inverted = true

    initialPage: Dienst.verknuepft
                 ? chatSeite
                 : kopplungsSeite

    Component { id: chatSeiteC; ChatsPage {} }

    ChatsPage { id: chatSeite }
    PairPage  { id: kopplungsSeite }

    // Nach dem Verknuepfen von selbst weiterschalten: die Kopplungsseite
    // hat dann nichts mehr zu zeigen.
    Connections {
        target: Dienst
        onStatusChanged: {
            if (Dienst.verknuepft && pageStack.currentPage === kopplungsSeite)
                pageStack.replace(chatSeite)
        }
    }
}
