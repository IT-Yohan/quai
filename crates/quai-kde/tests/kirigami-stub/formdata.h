#pragma once
#include <QObject>
#include <QString>
#include <QtQml/qqml.h>

// Kirigami.FormData attached properties (label, isSection, buddyFor).
class FormData : public QObject
{
    Q_OBJECT
    QML_ELEMENT
    QML_UNCREATABLE("FormData is an attached property")
    QML_ATTACHED(FormData)
    Q_PROPERTY(QString label MEMBER m_label NOTIFY changed)
    Q_PROPERTY(bool isSection MEMBER m_isSection NOTIFY changed)
    Q_PROPERTY(QObject *buddyFor MEMBER m_buddyFor NOTIFY changed)
public:
    using QObject::QObject;
    static FormData *qmlAttachedProperties(QObject *object) { return new FormData(object); }
Q_SIGNALS:
    void changed();
private:
    QString m_label;
    bool m_isSection = false;
    QObject *m_buddyFor = nullptr;
};

// Enumerations Kirigami exposes as types.
class MessageType : public QObject
{
    Q_OBJECT
    QML_ELEMENT
    QML_UNCREATABLE("enum holder")
public:
    enum Type { Information, Positive, Warning, Error };
    Q_ENUM(Type)
};

class DisplayHint : public QObject
{
    Q_OBJECT
    QML_ELEMENT
    QML_UNCREATABLE("enum holder")
public:
    enum Hint { NoPreference = 0, IconOnly = 1, KeepVisible = 2, AlwaysHide = 4, HideChildIndicator = 8 };
    Q_ENUM(Hint)
};

class ApplicationHeaderStyle : public QObject
{
    Q_OBJECT
    QML_ELEMENT
    QML_UNCREATABLE("enum holder")
public:
    enum NavigationButton { NoNavigationButtons = 0, ShowBackButton = 1, ShowForwardButton = 2 };
    Q_ENUM(NavigationButton)
};
