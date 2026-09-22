#include "Backend.h"
#include "Json.h"

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QProcess>
#include <QTimer>
#include <QDateTime>

// Dieselbe Portreihe wie im Dienst (siehe backend/src/main.rs). Das
// WhatsApp-Backend nimmt 8085-8089; um zehn verschoben laufen beide
// nebeneinander.
static const int ERSTER_PORT = 8095;
static const int LETZTER_PORT = 8099;

Backend::Backend(const QString &binary, QObject *parent)
    : QObject(parent)
    , m_netz(new QNetworkAccessManager(this))
    , m_ereignis(0)
    , m_dienst(0)
    , m_takt(new QTimer(this))
    , m_binary(binary)
    , m_port(0)
    , m_seq(-1)
    , m_verknuepft(false)
    , m_verbunden(false)
{
    m_zustand = QLatin1String("startet");
    // Der Takt ist der Rueckfall, nicht der Hauptweg: Neuigkeiten kommen
    // ueber /events. Zehn Sekunden genuegen, um einen abgerissenen
    // Langpoll zu bemerken.
    m_takt->setInterval(10000);
    connect(m_takt, SIGNAL(timeout()), this, SLOT(abfragen()));
}

Backend::~Backend()
{
    // Den Dienst laufen lassen: er nimmt Nachrichten entgegen, auch wenn
    // die App zu ist. Beendet wird er ueber /quit, nicht durch Zufall.
}

int Backend::port()
{
    return m_port ? m_port : ERSTER_PORT;
}

QUrl Backend::adresse(const QString &pfad)
{
    QUrl u;
    u.setScheme(QLatin1String("http"));
    u.setHost(QLatin1String("127.0.0.1"));
    u.setPort(port());
    u.setPath(pfad);
    return u;
}

QNetworkReply *Backend::hole(const QString &pfad)
{
    return m_netz->get(QNetworkRequest(adresse(pfad)));
}

QString Backend::kopplungsBild() const
{
    if (m_kopplungsAdresse.isEmpty())
        return QString();
    // Das Bild holt QML selbst ueber HTTP -- der Dienst malt es, und
    // niemand muss hier PNG auspacken.
    return QString::fromLatin1("http://127.0.0.1:%1/pair/qr")
            .arg(m_port ? m_port : ERSTER_PORT);
}

bool Backend::istGruppe(const QString &jid) const
{
    return jid.startsWith(QLatin1String("g:"));
}

void Backend::setzeFehler(const QString &text)
{
    if (m_fehler == text)
        return;
    m_fehler = text;
    emit fehlerChanged();
}

void Backend::starten()
{
    // Laeuft schon einer? Dann nur anklopfen. Der Dienst haelt sich selbst
    // an genau eine Instanz, aber ein zweiter Start kostet auf diesem
    // Geraet mehrere Sekunden.
    QNetworkReply *r = hole(QLatin1String("/status"));
    connect(r, SIGNAL(finished()), this, SLOT(statusFertig()));

    if (!m_dienst && QFile::exists(m_binary)) {
        m_dienst = new QProcess(this);
        // Ohne das faengt QProcess die Ausgabe ab und niemand sieht sie
        // je -- beim WhatsApp-Port hat das die Fehlersuche unnoetig
        // verlaengert.
        m_dienst->setProcessChannelMode(QProcess::ForwardedChannels);
        m_dienst->setWorkingDirectory(QDir::homePath());
        m_dienst->setEnvironment(QProcess::systemEnvironment());
        m_dienst->start(m_binary, QStringList());
    }
    m_takt->start();
    QTimer::singleShot(1500, this, SLOT(abfragen()));
}

void Backend::abfragen()
{
    QNetworkReply *r = hole(QLatin1String("/status"));
    connect(r, SIGNAL(finished()), this, SLOT(statusFertig()));
    ereignisPoll();
}

void Backend::statusFertig()
{
    QNetworkReply *r = qobject_cast<QNetworkReply *>(sender());
    if (!r)
        return;
    r->deleteLater();
    if (r->error() != QNetworkReply::NoError) {
        // Solange der Dienst hochfaehrt, ist das normal.
        if (m_zustand != QLatin1String("startet")) {
            m_verbunden = false;
            m_zustand = QLatin1String("kein Dienst");
            emit statusChanged();
        }
        return;
    }
    // Der belegte Port merken: beim naechsten Mal direkt dorthin.
    m_port = r->url().port(ERSTER_PORT);

    const QVariantMap s = Json::parse(QString::fromUtf8(r->readAll())).toMap();
    if (s.isEmpty())
        return;

    const bool verk = s.value(QLatin1String("paired")).toBool();
    const bool verb = s.value(QLatin1String("connected")).toBool();
    const QString nr = s.value(QLatin1String("phone")).toString();
    const QString adr = s.value(QLatin1String("pairUrl")).toString();
    const QString zus = s.value(QLatin1String("state")).toString();
    const QString feh = s.value(QLatin1String("lastError")).toString();

    QStringList ger;
    const QVariantList rohe = s.value(QLatin1String("devices")).toList();
    for (int i = 0; i < rohe.size(); ++i)
        ger.append(rohe.at(i).toString());

    if (verk != m_verknuepft || verb != m_verbunden || nr != m_nummer
            || adr != m_kopplungsAdresse || zus != m_zustand || ger != m_geraete) {
        const bool frischVerknuepft = verk && !m_verknuepft;
        m_verknuepft = verk;
        m_verbunden = verb;
        m_nummer = nr;
        m_kopplungsAdresse = adr;
        m_zustand = zus;
        m_geraete = ger;
        emit statusChanged();
        if (frischVerknuepft)
            neuLaden();
    }
    if (!feh.isEmpty())
        setzeFehler(feh);
}

void Backend::ereignisPoll()
{
    if (m_ereignis) {
        // Qt 4.7 kennt keine Zeitgrenze fuer Netzanfragen. Bleibt eine
        // Antwort aus, kommt finished() nie, der Merker bleibt belegt --
        // und danach erfaehrt die Oberflaeche nichts mehr.
        if (m_ereignisSeit.isValid() && m_ereignisSeit.elapsed() < 30000)
            return;
        QNetworkReply *alt = m_ereignis;
        m_ereignis = 0;
        alt->abort();
    }
    QUrl u = adresse(QLatin1String("/events"));
    u.addQueryItem(QLatin1String("since"), QString::number(m_seq));
    m_ereignis = m_netz->get(QNetworkRequest(u));
    m_ereignisSeit.start();
    connect(m_ereignis, SIGNAL(finished()), this, SLOT(ereignisFertig()));
}

void Backend::ereignisFertig()
{
    QNetworkReply *r = qobject_cast<QNetworkReply *>(sender());
    if (!r)
        return;
    if (r == m_ereignis)
        m_ereignis = 0;
    r->deleteLater();
    if (r->error() != QNetworkReply::NoError)
        return;
    const QVariantMap e = Json::parse(QString::fromUtf8(r->readAll())).toMap();
    const qint64 neu = e.value(QLatin1String("seq")).toLongLong();
    if (neu == m_seq)
        return;
    m_seq = neu;
    neuLaden();
}

void Backend::neuLaden()
{
    QNetworkReply *r = hole(QLatin1String("/chats"));
    connect(r, SIGNAL(finished()), this, SLOT(chatsFertig()));
    if (!m_offenerChat.isEmpty())
        chatOeffnen(m_offenerChat);
}

void Backend::chatsFertig()
{
    QNetworkReply *r = qobject_cast<QNetworkReply *>(sender());
    if (!r)
        return;
    r->deleteLater();
    if (r->error() != QNetworkReply::NoError)
        return;
    const QVariant v = Json::parse(QString::fromUtf8(r->readAll()));
    m_chats = v.toList();
    emit chatsChanged();
}

void Backend::chatOeffnen(const QString &jid)
{
    m_offenerChat = jid;
    QUrl u = adresse(QLatin1String("/messages"));
    u.addQueryItem(QLatin1String("jid"), jid);
    QNetworkReply *r = m_netz->get(QNetworkRequest(u));
    connect(r, SIGNAL(finished()), this, SLOT(nachrichtenFertig()));
}

void Backend::chatSchliessen()
{
    m_offenerChat.clear();
    m_nachrichten.clear();
    emit nachrichtenChanged();
}

void Backend::nachrichtenFertig()
{
    QNetworkReply *r = qobject_cast<QNetworkReply *>(sender());
    if (!r)
        return;
    r->deleteLater();
    if (r->error() != QNetworkReply::NoError)
        return;
    m_nachrichten = Json::parse(QString::fromUtf8(r->readAll())).toList();
    emit nachrichtenChanged();
}

void Backend::senden(const QString &jid, const QString &text)
{
    if (text.trimmed().isEmpty())
        return;
    QUrl u = adresse(QLatin1String("/send"));
    u.addQueryItem(QLatin1String("to"), jid);
    u.addQueryItem(QLatin1String("text"), text);
    QNetworkReply *r = m_netz->get(QNetworkRequest(u));
    connect(r, SIGNAL(finished()), this, SLOT(sendenFertig()));
}

void Backend::sendenFertig()
{
    QNetworkReply *r = qobject_cast<QNetworkReply *>(sender());
    if (!r)
        return;
    r->deleteLater();
    if (r->error() != QNetworkReply::NoError) {
        // Der Dienst schickt seinen Fehler als JSON; ist da keiner drin,
        // taugt wenigstens die Netzmeldung.
        const QString roh = QString::fromUtf8(r->readAll());
        const QString grund = Json::parse(roh).toMap()
                .value(QLatin1String("error")).toString();
        setzeFehler(grund.isEmpty() ? r->errorString() : grund);
        return;
    }
    setzeFehler(QString());
    // Der Dienst traegt die eigene Nachricht selbst ein -- gleich nachsehen,
    // damit sie nicht erst beim naechsten Ereignis erscheint.
    QTimer::singleShot(300, this, SLOT(abfragen()));
}

void Backend::koppeln()
{
    QNetworkReply *r = hole(QLatin1String("/pair"));
    connect(r, SIGNAL(finished()), this, SLOT(kopplungFertig()));
}

void Backend::kopplungFertig()
{
    QNetworkReply *r = qobject_cast<QNetworkReply *>(sender());
    if (!r)
        return;
    r->deleteLater();
    const QString roh = QString::fromUtf8(r->readAll());
    if (r->error() != QNetworkReply::NoError) {
        const QString grund = Json::parse(roh).toMap()
                .value(QLatin1String("error")).toString();
        setzeFehler(grund.isEmpty() ? r->errorString() : grund);
        return;
    }
    setzeFehler(QString());
    // Die Adresse kommt Sekunden spaeter; sie steht dann im Zustand.
    QTimer::singleShot(1000, this, SLOT(abfragen()));
}

QString Backend::zeit(const QVariant &wert) const
{
    const qint64 sek = wert.toLongLong();
    if (sek <= 0)
        return QString();
    const QDateTime t = QDateTime::fromTime_t((uint)sek);
    const QDate heute = QDate::currentDate();
    if (t.date() == heute)
        return t.toString(QLatin1String("HH:mm"));
    if (t.date().daysTo(heute) < 7)
        return t.toString(QLatin1String("ddd HH:mm"));
    return t.toString(QLatin1String("dd.MM. HH:mm"));
}
