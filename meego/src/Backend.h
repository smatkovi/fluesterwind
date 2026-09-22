#ifndef BACKEND_H
#define BACKEND_H

// Die Verbindung der Oberflaeche zum Signal-Dienst.
//
// Der Dienst ist in Rust geschrieben und spricht HTTP auf 127.0.0.1 --
// dieselbe Schnittstelle wie das Go-Backend des WhatsApp-Ports fuer
// dasselbe Geraet: /status, /pair, /chats, /messages, /send, /events.
// Deshalb liess sich diese Klasse von dort uebernehmen.
//
// Uebernommen ist auch, was dort Muehe gekostet hat:
//
//  * Genau ein /events-Langpoll darf offen sein. Sammelten sie sich an,
//    lief Qt 4.7 in seine Grenze von sechs Verbindungen je Gegenstelle,
//    und /messages blieb in der Warteschlange haengen -- die Chatliste
//    wurde dann ohne Vorwarnung leer.
//  * Parameter gehoeren per addQueryItem an die Adresse, nicht von Hand
//    kodiert: sonst kommt beim Empfaenger "%2C" statt eines Kommas an.
//  * Jede Abfrage braucht eine Zeitgrenze. Qt 4.7 kennt keine, und eine
//    haengende Antwort legte im WhatsApp-Port die ganze Anrufansicht
//    still.

#include <QObject>
#include <QString>
#include <QStringList>
#include <QTime>
#include <QUrl>
#include <QVariantList>
#include <QVariantMap>

class QNetworkAccessManager;
class QNetworkReply;
class QProcess;
class QTimer;

class Backend : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QString zustand READ zustand NOTIFY statusChanged)
    Q_PROPERTY(bool verknuepft READ verknuepft NOTIFY statusChanged)
    Q_PROPERTY(bool verbunden READ verbunden NOTIFY statusChanged)
    Q_PROPERTY(QString nummer READ nummer NOTIFY statusChanged)
    Q_PROPERTY(QString kopplungsAdresse READ kopplungsAdresse NOTIFY statusChanged)
    // Die Adresse, unter der der Dienst den Kopplungscode als Bild malt.
    // Leer, solange keine Kopplung laeuft.
    Q_PROPERTY(QString kopplungsBild READ kopplungsBild NOTIFY statusChanged)
    Q_PROPERTY(QStringList geraete READ geraete NOTIFY statusChanged)
    Q_PROPERTY(QString fehler READ fehler NOTIFY fehlerChanged)
    Q_PROPERTY(QVariantList chats READ chats NOTIFY chatsChanged)
    Q_PROPERTY(QVariantList nachrichten READ nachrichten NOTIFY nachrichtenChanged)
    Q_PROPERTY(QString offenerChat READ offenerChat NOTIFY nachrichtenChanged)

public:
    explicit Backend(const QString &binary, QObject *parent = 0);
    ~Backend();

    QString zustand() const { return m_zustand; }
    bool verknuepft() const { return m_verknuepft; }
    bool verbunden() const { return m_verbunden; }
    QString nummer() const { return m_nummer; }
    QString kopplungsAdresse() const { return m_kopplungsAdresse; }
    QString kopplungsBild() const;
    QStringList geraete() const { return m_geraete; }
    QString fehler() const { return m_fehler; }
    QVariantList chats() const { return m_chats; }
    QVariantList nachrichten() const { return m_nachrichten; }
    QString offenerChat() const { return m_offenerChat; }

    Q_INVOKABLE void starten();
    Q_INVOKABLE void koppeln();
    Q_INVOKABLE void chatOeffnen(const QString &jid);
    Q_INVOKABLE void chatSchliessen();
    Q_INVOKABLE void senden(const QString &jid, const QString &text);
    Q_INVOKABLE void neuLaden();
    Q_INVOKABLE QString zeit(const QVariant &wert) const;
    // Eine Gruppe erkennt man am Praefix, nicht an der Laenge der
    // Kennung. Beim WhatsApp-Port entschied die Laenge, und das ging
    // schief, sobald eine Kennung genau an der Grenze lag.
    Q_INVOKABLE bool istGruppe(const QString &jid) const;

signals:
    void statusChanged();
    void chatsChanged();
    void nachrichtenChanged();
    void fehlerChanged();

private slots:
    void statusFertig();
    void chatsFertig();
    void nachrichtenFertig();
    void sendenFertig();
    void kopplungFertig();
    void ereignisFertig();
    void abfragen();

private:
    QNetworkReply *hole(const QString &pfad);
    QUrl adresse(const QString &pfad);
    void ereignisPoll();
    void setzeFehler(const QString &text);
    int port();

    QNetworkAccessManager *m_netz;
    QNetworkReply *m_ereignis;   // hoechstens einer offen
    QTime m_ereignisSeit;
    QProcess *m_dienst;
    QTimer *m_takt;
    QString m_binary;
    int m_port;
    qint64 m_seq;

    QString m_zustand;
    bool m_verknuepft;
    bool m_verbunden;
    QString m_nummer;
    QString m_kopplungsAdresse;
    QStringList m_geraete;
    QString m_fehler;
    QVariantList m_chats;
    QVariantList m_nachrichten;
    QString m_offenerChat;
};

#endif
