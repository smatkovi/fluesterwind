// Signal fuer MeeGo Harmattan (Fluesterwind) (Nokia N9/N950).
//
// Die Oberflaeche stammt aus dem WhatsApp-Port fuer dasselbe Geraet; was
// sie bedient, ist ein anderer Dienst. Beide sprechen dieselbe
// HTTP-Schnittstelle auf 127.0.0.1 -- deshalb liess sich das uebernehmen,
// statt es ein zweites Mal zu schreiben.
//
// Whisperfish selbst kam als Vorlage nicht in Frage: dessen Oberflaeche
// ist Silica-QML, von Rust ueber qmetaobject-rs gegen Qt 5 getrieben.
// Harmattan hat Qt 4.7 und kein Silica.

#include <QApplication>
#include <QDeclarativeContext>
#include <QDeclarativeEngine>
#include <QDeclarativeView>
#include <QDir>
#include <QFile>
#include <QInputContext>
#include <QInputContextFactory>
#include <QFileInfo>

#include "src/Backend.h"

// Harmattan schreibt die Adresse des Sitzungsbusses hierhin. Ein per ssh
// oder aus einem Dienst gestarteter Prozess erbt sie nicht -- und ohne sie
// scheitert alles, was ueber den Bus laeuft. Sichtbar wurde das beim
// Abspielen einer Sprachnachricht: xdg-open reicht die Datei ueber
// libcontentaction an die Musik-App weiter und meldete nur
// "Not connected to D-Bus server", ohne dass etwas passierte.
static void sitzungsBusSetzen()
{
    if (!qgetenv("DBUS_SESSION_BUS_ADDRESS").isEmpty())
        return;
    QFile f(QLatin1String("/tmp/session_bus_address.user"));
    if (!f.open(QIODevice::ReadOnly))
        return;
    const QStringList zeilen = QString::fromLatin1(f.readAll()).split(QLatin1Char('\n'));
    for (int i = 0; i < zeilen.size(); ++i) {
        const QString z = zeilen.at(i);
        const int p = z.indexOf(QLatin1String("DBUS_SESSION_BUS_ADDRESS="));
        if (p < 0)
            continue;
        QString wert = z.mid(p + 25).trimmed();
        if (wert.endsWith(QLatin1Char(';')))
            wert.chop(1);
        if (wert.length() > 1 && (wert.at(0) == QLatin1Char('"') || wert.at(0) == QLatin1Char('\''))
                && wert.at(wert.length() - 1) == wert.at(0)) {
            wert = wert.mid(1, wert.length() - 2);
        }
        if (!wert.isEmpty())
            qputenv("DBUS_SESSION_BUS_ADDRESS", wert.toLatin1());
        return;
    }
}

int main(int argc, char *argv[])
{
    sitzungsBusSetzen();
    QApplication app(argc, argv);

    // Ohne das bleibt die virtuelle Tastatur weg, sobald die ausziehbare
    // eingeklappt ist: Qt waehlt dann gar keinen Eingabekontext, und ein
    // TextField bekommt zwar den Fokus, aber nichts erscheint. Harmattans
    // Tastatur haengt an MInputContext, und die Standardapps setzen ihn
    // ueber ihre Bibliotheken -- eine nackte QApplication tut das nicht.
    if (QInputContext *ic = QInputContextFactory::create(
            QLatin1String("MInputContext"), &app)) {
        app.setInputContext(ic);
    }

    app.setApplicationName(QLatin1String("fluesterwind"));
    app.setOrganizationName(QLatin1String("fluesterwind"));

    // Alles liegt relativ zum Programm, damit sich das Paket verschieben
    // laesst, ohne dass Pfade nachgezogen werden muessen.
    const QString wurzel = QFileInfo(QCoreApplication::applicationFilePath())
            .absolutePath() + QLatin1String("/..");

    Backend backend(QDir(wurzel).absoluteFilePath(QLatin1String("bin/signal-backend")));

    QDeclarativeView view;
    view.engine()->rootContext()->setContextProperty(QLatin1String("Dienst"), &backend);
    view.setResizeMode(QDeclarativeView::SizeRootObjectToView);
    view.setSource(QUrl::fromLocalFile(
        QDir(wurzel).absoluteFilePath(QLatin1String("qml/main.qml"))));
    view.showFullScreen();

    backend.starten();
    return app.exec();
}
