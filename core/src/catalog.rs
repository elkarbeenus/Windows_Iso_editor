//! Everything the user can pick. Each entry knows
//!   * how it is shown (German + English text, what it really does),
//!   * in which preset it is active,
//!   * the exact actions that end up in the setup scripts.
//!
//! Golden rule: nothing here touches gaming (Xbox, Game Bar, Gaming Services),
//! Bluetooth, drivers, the Microsoft Store, runtimes (.NET, VC++, WebView2),
//! Windows Update or Windows Security. Those are additionally protected by a
//! hard-coded list inside the generated script.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Preset {
    Gentle,
    Recommended,
    Maximum,
}

pub enum Action {
    /// Remove provisioned Store apps (by package family display name).
    Appx(&'static [&'static str]),
    /// Remove Windows capabilities (prefix match on capability name).
    Capability(&'static [&'static str]),
    /// Disable optional features.
    Feature(&'static [&'static str]),
    /// PowerShell running as SYSTEM in the specialize pass.
    System(&'static str),
    /// PowerShell running in specialize with the default user hive loaded at
    /// `HKU\DefaultUser` (applies to every account created later).
    DefaultUser(&'static str),
    /// reg.exe commands executed in Windows PE before setup checks hardware.
    WinPe(&'static [&'static str]),
}

pub struct Item {
    pub id: &'static str,
    pub group: &'static str,
    pub icon: &'static str,
    pub title_de: &'static str,
    pub title_en: &'static str,
    pub desc_de: &'static str,
    pub desc_en: &'static str,
    /// Technical details: what exactly is changed.
    pub tech: &'static str,
    /// Optional warning shown on the card.
    pub warn_de: &'static str,
    pub warn_en: &'static str,
    /// [gentle, recommended, maximum]
    pub presets: [bool; 3],
    pub actions: &'static [Action],
}

pub struct Group {
    pub id: &'static str,
    pub icon: &'static str,
    pub title_de: &'static str,
    pub title_en: &'static str,
    pub desc_de: &'static str,
    pub desc_en: &'static str,
}

pub const GROUPS: &[Group] = &[
    Group { id: "ai", icon: "sparkles", title_de: "KI & Überwachung", title_en: "AI & surveillance",
        desc_de: "Copilot, Recall, Cortana und andere KI-Funktionen", desc_en: "Copilot, Recall, Cortana and other AI features" },
    Group { id: "privacy", icon: "shield", title_de: "Datenschutz", title_en: "Privacy",
        desc_de: "Telemetrie, Werbe-ID, Websuche, Aktivitätsverlauf", desc_en: "Telemetry, advertising ID, web search, activity history" },
    Group { id: "ads", icon: "megaphone", title_de: "Werbung & Vorschläge", title_en: "Ads & suggestions",
        desc_de: "Gesponserte Apps, Werbung im Startmenü, Widgets", desc_en: "Sponsored apps, Start menu ads, widgets" },
    Group { id: "apps", icon: "grid", title_de: "Vorinstallierte Apps", title_en: "Preinstalled apps",
        desc_de: "Apps, die kaum jemand braucht – jede einzeln wählbar", desc_en: "Apps hardly anyone needs – each one selectable" },
    Group { id: "setup", icon: "disc", title_de: "Installation", title_en: "Installation",
        desc_de: "Hilfen für die Einrichtung", desc_en: "Setup helpers" },
    Group { id: "comfort", icon: "sliders", title_de: "Komfort", title_en: "Comfort",
        desc_de: "Kleine Verbesserungen für den Alltag", desc_en: "Small everyday improvements" },
];

/// Packages that are never removed, no matter what (checked inside the script too).
pub const PROTECTED: &[&str] = &[
    "Microsoft.WindowsStore",
    "Microsoft.StorePurchaseApp",
    "Microsoft.DesktopAppInstaller",
    "Microsoft.GamingApp",
    "Microsoft.GamingServices",
    "Microsoft.XboxGamingOverlay",
    "Microsoft.XboxGameOverlay",
    "Microsoft.XboxIdentityProvider",
    "Microsoft.XboxSpeechToTextOverlay",
    "Microsoft.Xbox.TCUI",
    "Microsoft.XboxApp",
    "Microsoft.SecHealthUI",
    "Microsoft.WindowsTerminal",
    "Microsoft.WindowsNotepad",
    "Microsoft.WindowsCalculator",
    "Microsoft.Paint",
    "Microsoft.ScreenSketch",
    "Microsoft.Windows.Photos",
    "Microsoft.WindowsCamera",
    "Microsoft.VCLibs*",
    "Microsoft.UI.Xaml*",
    "Microsoft.NET*",
    "Microsoft.WindowsAppRuntime*",
    "Microsoft.WebView2*",
    "Microsoft.*Extension*",
    "Microsoft.ApplicationCompatibilityEnhancements",
    "Microsoft.Winget*",
    "MicrosoftWindows.Client.CBS",
    "Microsoft.Windows.ShellExperienceHost",
    "Microsoft.Windows.StartMenuExperienceHost",
];

/// Shown in the UI as "stays guaranteed".
pub const PROTECTED_LABELS_DE: &[&str] = &[
    "Xbox-App, Game Bar & Gaming Services",
    "Bluetooth, WLAN & alle Treiber",
    "Microsoft Store & winget",
    "Windows Update & Windows-Sicherheit",
    ".NET, Visual C++, WebView2 & Codecs",
    "Edge (nur Werbung wird abgeschaltet)",
    "Rechner, Editor, Paint, Fotos, Kamera, Snipping Tool, Terminal",
];
pub const PROTECTED_LABELS_EN: &[&str] = &[
    "Xbox app, Game Bar & Gaming Services",
    "Bluetooth, Wi-Fi & all drivers",
    "Microsoft Store & winget",
    "Windows Update & Windows Security",
    ".NET, Visual C++, WebView2 & codecs",
    "Edge (only its ads are turned off)",
    "Calculator, Notepad, Paint, Photos, Camera, Snipping Tool, Terminal",
];

const ALL: [bool; 3] = [true, true, true];
const REC: [bool; 3] = [false, true, true];
const MAX: [bool; 3] = [false, false, true];
const OFF: [bool; 3] = [false, false, false];

pub const ITEMS: &[Item] = &[
    // ------------------------------------------------------------ AI
    Item {
        id: "copilot", group: "ai", icon: "bot",
        title_de: "Copilot entfernen", title_en: "Remove Copilot",
        desc_de: "Entfernt die Copilot-App und schaltet Copilot per Richtlinie ab – auch die Copilot-Seitenleiste in Edge.",
        desc_en: "Removes the Copilot app and turns Copilot off by policy – including the Copilot sidebar in Edge.",
        tech: "Appx: Microsoft.Copilot, Microsoft.Windows.Ai.Copilot.Provider · HKLM/HKCU Policies\\WindowsCopilot TurnOffWindowsCopilot=1 · ShowCopilotButton=0 · Edge HubsSidebarEnabled=0",
        warn_de: "", warn_en: "",
        presets: ALL,
        actions: &[
            Action::Appx(&["Microsoft.Copilot", "Microsoft.Windows.Ai.Copilot.Provider", "Microsoft.MicrosoftCopilot"]),
            Action::System(r#"RegSet 'HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsCopilot' 'TurnOffWindowsCopilot' 1
RegSet 'HKLM\SOFTWARE\Policies\Microsoft\Edge' 'HubsSidebarEnabled' 0"#),
            Action::DefaultUser(r#"RegSet 'HKU\DefaultUser\Software\Policies\Microsoft\Windows\WindowsCopilot' 'TurnOffWindowsCopilot' 1
RegSet 'HKU\DefaultUser\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced' 'ShowCopilotButton' 0"#),
        ],
    },
    Item {
        id: "recall", group: "ai", icon: "eye-off",
        title_de: "Recall & „Click to Do“ abschalten", title_en: "Turn off Recall & Click to Do",
        desc_de: "Recall macht laufend Screenshots von allem, was du tust. Das Feature wird deaktiviert und per Richtlinie gesperrt.",
        desc_en: "Recall constantly takes screenshots of everything you do. The feature is disabled and locked by policy.",
        tech: "Optional feature 'Recall' disabled · Policies\\WindowsAI DisableAIDataAnalysis=1, AllowRecallEnablement=0, DisableClickToDo=1 (HKLM + HKCU)",
        warn_de: "", warn_en: "",
        presets: ALL,
        actions: &[
            Action::Feature(&["Recall"]),
            Action::System(r#"RegSet 'HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsAI' 'DisableAIDataAnalysis' 1
RegSet 'HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsAI' 'AllowRecallEnablement' 0
RegSet 'HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsAI' 'DisableClickToDo' 1"#),
            Action::DefaultUser(r#"RegSet 'HKU\DefaultUser\Software\Policies\Microsoft\Windows\WindowsAI' 'DisableAIDataAnalysis' 1
RegSet 'HKU\DefaultUser\Software\Policies\Microsoft\Windows\WindowsAI' 'DisableClickToDo' 1"#),
        ],
    },
    Item {
        id: "cortana", group: "ai", icon: "mic-off",
        title_de: "Cortana entfernen", title_en: "Remove Cortana",
        desc_de: "Entfernt die Cortana-App (bei älteren Builds noch dabei) und verbietet Cortana in der Suche.",
        desc_en: "Removes the Cortana app (still shipped with older builds) and disables Cortana in search.",
        tech: "Appx: Microsoft.549981C3F5F10 · Policies\\Windows Search AllowCortana=0",
        warn_de: "", warn_en: "",
        presets: ALL,
        actions: &[
            Action::Appx(&["Microsoft.549981C3F5F10"]),
            Action::System(r#"RegSet 'HKLM\SOFTWARE\Policies\Microsoft\Windows\Windows Search' 'AllowCortana' 0"#),
        ],
    },
    Item {
        id: "ai_apps", group: "ai", icon: "wand",
        title_de: "KI in Paint & Editor abschalten", title_en: "Turn off AI in Paint & Notepad",
        desc_de: "Schaltet Image Creator, Cocreator, generatives Füllen (Paint) und die KI-Funktionen im Editor ab. Die Apps bleiben.",
        desc_en: "Turns off Image Creator, Cocreator, generative fill (Paint) and the AI features in Notepad. The apps stay.",
        tech: "HKLM ...\\Policies\\Paint DisableImageCreator/DisableCocreator/DisableGenerativeFill=1 · HKLM Policies\\WindowsNotepad DisableAIFeatures=1",
        warn_de: "", warn_en: "",
        presets: REC,
        actions: &[Action::System(r#"RegSet 'HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\Paint' 'DisableImageCreator' 1
RegSet 'HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\Paint' 'DisableCocreator' 1
RegSet 'HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\Paint' 'DisableGenerativeFill' 1
RegSet 'HKLM\SOFTWARE\Policies\WindowsNotepad' 'DisableAIFeatures' 1"#)],
    },
    // ------------------------------------------------------------ privacy
    Item {
        id: "telemetry", group: "privacy", icon: "radio",
        title_de: "Telemetrie minimieren", title_en: "Minimize telemetry",
        desc_de: "Diagnosedaten auf das Minimum, keine „maßgeschneiderten Erlebnisse“, keine Feedback-Nachfragen.",
        desc_en: "Diagnostic data set to the minimum, no “tailored experiences”, no feedback prompts.",
        tech: "Policies\\DataCollection AllowTelemetry=1, DoNotShowFeedbackNotifications=1, LimitDiagnosticLogCollection=1 · HKCU Siuf NumberOfSIUFInPeriod=0, TailoredExperiences…=0",
        warn_de: "", warn_en: "",
        presets: REC,
        actions: &[
            Action::System(r#"RegSet 'HKLM\SOFTWARE\Policies\Microsoft\Windows\DataCollection' 'AllowTelemetry' 1
RegSet 'HKLM\SOFTWARE\Policies\Microsoft\Windows\DataCollection' 'DoNotShowFeedbackNotifications' 1
RegSet 'HKLM\SOFTWARE\Policies\Microsoft\Windows\DataCollection' 'LimitDiagnosticLogCollection' 1
RegSet 'HKLM\SOFTWARE\Policies\Microsoft\Windows\DataCollection' 'DisableOneSettingsDownloads' 1"#),
            Action::DefaultUser(r#"RegSet 'HKU\DefaultUser\Software\Microsoft\Siuf\Rules' 'NumberOfSIUFInPeriod' 0
RegSet 'HKU\DefaultUser\Software\Microsoft\Windows\CurrentVersion\Privacy' 'TailoredExperiencesWithDiagnosticDataEnabled' 0"#),
        ],
    },
    Item {
        id: "diagtrack", group: "privacy", icon: "power",
        title_de: "Telemetrie-Dienste deaktivieren", title_en: "Disable telemetry services",
        desc_de: "Schaltet die Dienste „Benutzererfahrung und Telemetrie“ (DiagTrack) und dmwappushservice ab.",
        desc_en: "Disables the “Connected User Experiences and Telemetry” (DiagTrack) and dmwappushservice services.",
        tech: "Services DiagTrack + dmwappushservice → Start=Disabled",
        warn_de: "", warn_en: "",
        presets: MAX,
        actions: &[Action::System(r#"foreach ($s in 'DiagTrack','dmwappushservice') { if (Get-Service $s -ErrorAction SilentlyContinue) { Set-Service -Name $s -StartupType Disabled } }"#)],
    },
    Item {
        id: "adid", group: "privacy", icon: "fingerprint",
        title_de: "Werbe-ID abschalten", title_en: "Turn off advertising ID",
        desc_de: "Apps können dich nicht mehr über eine Werbe-ID wiedererkennen.",
        desc_en: "Apps can no longer recognize you through an advertising ID.",
        tech: "Policies\\AdvertisingInfo DisabledByGroupPolicy=1",
        warn_de: "", warn_en: "",
        presets: ALL,
        actions: &[Action::System(r#"RegSet 'HKLM\SOFTWARE\Policies\Microsoft\Windows\AdvertisingInfo' 'DisabledByGroupPolicy' 1"#)],
    },
    Item {
        id: "activity", group: "privacy", icon: "history",
        title_de: "Aktivitätsverlauf abschalten", title_en: "Turn off activity history",
        desc_de: "Windows speichert und überträgt nicht mehr, welche Programme und Dateien du benutzt.",
        desc_en: "Windows no longer stores and uploads which programs and files you use.",
        tech: "Policies\\System EnableActivityFeed=0, PublishUserActivities=0, UploadUserActivities=0",
        warn_de: "", warn_en: "",
        presets: REC,
        actions: &[Action::System(r#"RegSet 'HKLM\SOFTWARE\Policies\Microsoft\Windows\System' 'EnableActivityFeed' 0
RegSet 'HKLM\SOFTWARE\Policies\Microsoft\Windows\System' 'PublishUserActivities' 0
RegSet 'HKLM\SOFTWARE\Policies\Microsoft\Windows\System' 'UploadUserActivities' 0"#)],
    },
    Item {
        id: "websearch", group: "privacy", icon: "search",
        title_de: "Bing-Websuche im Startmenü aus", title_en: "No Bing web search in Start",
        desc_de: "Die Suche im Startmenü findet nur noch Dinge auf deinem PC – nichts wird an Bing geschickt.",
        desc_en: "Start menu search only finds things on your PC – nothing is sent to Bing.",
        tech: "Policies\\Windows Search DisableWebSearch=1, ConnectedSearchUseWeb=0 · HKCU Policies\\Explorer DisableSearchBoxSuggestions=1 · BingSearchEnabled=0",
        warn_de: "", warn_en: "",
        presets: ALL,
        actions: &[
            Action::System(r#"RegSet 'HKLM\SOFTWARE\Policies\Microsoft\Windows\Windows Search' 'DisableWebSearch' 1
RegSet 'HKLM\SOFTWARE\Policies\Microsoft\Windows\Windows Search' 'ConnectedSearchUseWeb' 0"#),
            Action::DefaultUser(r#"RegSet 'HKU\DefaultUser\Software\Policies\Microsoft\Windows\Explorer' 'DisableSearchBoxSuggestions' 1
RegSet 'HKU\DefaultUser\Software\Microsoft\Windows\CurrentVersion\Search' 'BingSearchEnabled' 0"#),
        ],
    },
    // ------------------------------------------------------------ ads
    Item {
        id: "sponsored", group: "ads", icon: "ban",
        title_de: "Keine gesponserten Apps", title_en: "No sponsored apps",
        desc_de: "Verhindert, dass Windows heimlich Candy Crush, TikTok & Co. nachinstalliert.",
        desc_en: "Stops Windows from silently installing Candy Crush, TikTok & co.",
        tech: "Policies\\CloudContent DisableWindowsConsumerFeatures=1 · HKCU ContentDeliveryManager: SilentInstalledAppsEnabled, PreInstalledAppsEnabled, OemPreInstalledAppsEnabled, ContentDeliveryAllowed … = 0",
        warn_de: "", warn_en: "",
        presets: ALL,
        actions: &[
            Action::System(r#"RegSet 'HKLM\SOFTWARE\Policies\Microsoft\Windows\CloudContent' 'DisableWindowsConsumerFeatures' 1
RegSet 'HKLM\SOFTWARE\Policies\Microsoft\Windows\CloudContent' 'DisableConsumerAccountStateContent' 1"#),
            Action::DefaultUser(r#"$cdm = 'HKU\DefaultUser\Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager'
foreach ($v in 'ContentDeliveryAllowed','FeatureManagementEnabled','OemPreInstalledAppsEnabled','PreInstalledAppsEnabled','PreInstalledAppsEverEnabled','SilentInstalledAppsEnabled','SoftLandingEnabled','SubscribedContentEnabled','SystemPaneSuggestionsEnabled') { RegSet $cdm $v 0 }"#),
        ],
    },
    Item {
        id: "startads", group: "ads", icon: "layout",
        title_de: "Werbung in Start, Explorer & Sperrbildschirm aus", title_en: "No ads in Start, Explorer & lock screen",
        desc_de: "Entfernt „Empfehlungen“, Konto-Hinweise, OneDrive-Werbung im Explorer, Tipps auf dem Sperrbildschirm und den „Gerät fertig einrichten“-Bildschirm.",
        desc_en: "Removes “recommendations”, account nags, OneDrive ads in Explorer, lock screen tips and the “finish setting up your device” screen.",
        tech: "HKCU Explorer\\Advanced Start_IrisRecommendations=0, Start_AccountNotifications=0, ShowSyncProviderNotifications=0 · CDM RotatingLockScreenOverlayEnabled=0, SubscribedContent-3383xx=0 · UserProfileEngagement ScoobeSystemSettingEnabled=0",
        warn_de: "", warn_en: "",
        presets: ALL,
        actions: &[Action::DefaultUser(r#"$adv = 'HKU\DefaultUser\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced'
RegSet $adv 'Start_IrisRecommendations' 0
RegSet $adv 'Start_AccountNotifications' 0
RegSet $adv 'ShowSyncProviderNotifications' 0
$cdm = 'HKU\DefaultUser\Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager'
foreach ($v in 'RotatingLockScreenOverlayEnabled','SubscribedContent-338387Enabled','SubscribedContent-338388Enabled','SubscribedContent-338389Enabled','SubscribedContent-310093Enabled','SubscribedContent-353694Enabled','SubscribedContent-353696Enabled') { RegSet $cdm $v 0 }
RegSet 'HKU\DefaultUser\Software\Microsoft\Windows\CurrentVersion\UserProfileEngagement' 'ScoobeSystemSettingEnabled' 0"#)],
    },
    Item {
        id: "widgets", group: "ads", icon: "newspaper",
        title_de: "Widgets & News entfernen", title_en: "Remove widgets & news",
        desc_de: "Entfernt das Widgets-Board mit MSN-News und Werbung aus der Taskleiste.",
        desc_en: "Removes the widgets board with MSN news and ads from the taskbar.",
        tech: "Appx: MicrosoftWindows.Client.WebExperience · Policies\\Dsh AllowNewsAndInterests=0",
        warn_de: "", warn_en: "",
        presets: REC,
        actions: &[
            Action::Appx(&["MicrosoftWindows.Client.WebExperience"]),
            Action::System(r#"RegSet 'HKLM\SOFTWARE\Policies\Microsoft\Dsh' 'AllowNewsAndInterests' 0"#),
        ],
    },
    Item {
        id: "edge", group: "ads", icon: "globe",
        title_de: "Edge entrümpeln (Edge bleibt)", title_en: "Declutter Edge (Edge stays)",
        desc_de: "Edge wird NICHT entfernt (das würde Windows beschädigen), aber Einkaufshilfe, Empfehlungen, Startup-Boost und Erststart-Assistent werden abgeschaltet.",
        desc_en: "Edge is NOT removed (that would damage Windows), but shopping assistant, recommendations, startup boost and first-run wizard are turned off.",
        tech: "Policies\\Edge EdgeShoppingAssistantEnabled=0, ShowRecommendationsEnabled=0, PersonalizationReportingEnabled=0, StartupBoostEnabled=0, HideFirstRunExperience=1",
        warn_de: "", warn_en: "",
        presets: REC,
        actions: &[Action::System(r#"$e = 'HKLM\SOFTWARE\Policies\Microsoft\Edge'
RegSet $e 'EdgeShoppingAssistantEnabled' 0
RegSet $e 'ShowRecommendationsEnabled' 0
RegSet $e 'PersonalizationReportingEnabled' 0
RegSet $e 'StartupBoostEnabled' 0
RegSet $e 'HideFirstRunExperience' 1"#)],
    },
    Item {
        id: "teams", group: "ads", icon: "message",
        title_de: "Teams & Chat-Symbol entfernen", title_en: "Remove Teams & chat icon",
        desc_de: "Entfernt Teams (privat) und das Chat-Symbol in der Taskleiste.",
        desc_en: "Removes Teams (personal) and the chat icon on the taskbar.",
        tech: "Appx: MSTeams, MicrosoftTeams · Policies\\Windows Chat ChatIcon=3",
        warn_de: "", warn_en: "",
        presets: REC,
        actions: &[
            Action::Appx(&["MSTeams", "MicrosoftTeams"]),
            Action::System(r#"RegSet 'HKLM\SOFTWARE\Policies\Microsoft\Windows\Windows Chat' 'ChatIcon' 3"#),
        ],
    },
    Item {
        id: "outlook_devhome", group: "ads", icon: "mail",
        title_de: "Neues Outlook & Dev Home entfernen", title_en: "Remove new Outlook & Dev Home",
        desc_de: "Entfernt beide Apps und verhindert, dass Windows sie nach der Installation automatisch wieder herunterlädt.",
        desc_en: "Removes both apps and stops Windows from downloading them again after installation.",
        tech: "Appx: Microsoft.OutlookForWindows, Microsoft.Windows.DevHome · delete HKLM\\…\\Orchestrator\\UScheduler_Oobe\\OutlookUpdate + DevHomeUpdate",
        warn_de: "", warn_en: "",
        presets: REC,
        actions: &[
            Action::Appx(&["Microsoft.OutlookForWindows", "Microsoft.Windows.DevHome"]),
            Action::System(r#"foreach ($k in 'OutlookUpdate','DevHomeUpdate') { reg.exe delete "HKLM\SOFTWARE\Microsoft\WindowsUpdate\Orchestrator\UScheduler_Oobe\$k" /f 2>$null | Out-Null }"#),
        ],
    },
    // ------------------------------------------------------------ apps
    Item { id: "app_news", group: "apps", icon: "newspaper", title_de: "News & Wetter", title_en: "News & Weather",
        desc_de: "MSN-News- und Wetter-App.", desc_en: "MSN News and Weather apps.",
        tech: "Appx: Microsoft.BingNews, Microsoft.BingWeather", warn_de: "", warn_en: "",
        presets: REC, actions: &[Action::Appx(&["Microsoft.BingNews", "Microsoft.BingWeather"])] },
    Item { id: "app_bing", group: "apps", icon: "search", title_de: "Microsoft Bing", title_en: "Microsoft Bing",
        desc_de: "Bing-Such-App.", desc_en: "Bing search app.",
        tech: "Appx: Microsoft.BingSearch", warn_de: "", warn_en: "",
        presets: REC, actions: &[Action::Appx(&["Microsoft.BingSearch"])] },
    Item { id: "app_tips", group: "apps", icon: "lightbulb", title_de: "Tipps & „Hilfe anfordern“", title_en: "Tips & Get Help",
        desc_de: "Einführungs- und Hilfe-Apps.", desc_en: "Intro and help apps.",
        tech: "Appx: Microsoft.Getstarted, Microsoft.GetHelp", warn_de: "", warn_en: "",
        presets: REC, actions: &[Action::Appx(&["Microsoft.Getstarted", "Microsoft.GetHelp"])] },
    Item { id: "app_officehub", group: "apps", icon: "briefcase", title_de: "Microsoft 365 (Office-Werbe-App)", title_en: "Microsoft 365 (Office promo app)",
        desc_de: "Die vorinstallierte „Microsoft 365 Copilot“-Werbe-App, nicht Office selbst.", desc_en: "The preinstalled “Microsoft 365 Copilot” promo app, not Office itself.",
        tech: "Appx: Microsoft.MicrosoftOfficeHub", warn_de: "", warn_en: "",
        presets: REC, actions: &[Action::Appx(&["Microsoft.MicrosoftOfficeHub"])] },
    Item { id: "app_solitaire", group: "apps", icon: "cards", title_de: "Solitaire Collection", title_en: "Solitaire Collection",
        desc_de: "Kartenspiele mit Werbung.", desc_en: "Card games with ads.",
        tech: "Appx: Microsoft.MicrosoftSolitaireCollection", warn_de: "", warn_en: "",
        presets: REC, actions: &[Action::Appx(&["Microsoft.MicrosoftSolitaireCollection"])] },
    Item { id: "app_clipchamp", group: "apps", icon: "film", title_de: "Clipchamp", title_en: "Clipchamp",
        desc_de: "Online-Videoeditor.", desc_en: "Online video editor.",
        tech: "Appx: Clipchamp.Clipchamp", warn_de: "", warn_en: "",
        presets: REC, actions: &[Action::Appx(&["Clipchamp.Clipchamp"])] },
    Item { id: "app_todo", group: "apps", icon: "check", title_de: "Microsoft To Do", title_en: "Microsoft To Do",
        desc_de: "Aufgaben-App mit Microsoft-Konto-Zwang.", desc_en: "Task app that needs a Microsoft account.",
        tech: "Appx: Microsoft.Todos", warn_de: "", warn_en: "",
        presets: REC, actions: &[Action::Appx(&["Microsoft.Todos"])] },
    Item { id: "app_feedback", group: "apps", icon: "message", title_de: "Feedback-Hub", title_en: "Feedback Hub",
        desc_de: "Sendet Feedback an Microsoft.", desc_en: "Sends feedback to Microsoft.",
        tech: "Appx: Microsoft.WindowsFeedbackHub", warn_de: "", warn_en: "",
        presets: REC, actions: &[Action::Appx(&["Microsoft.WindowsFeedbackHub"])] },
    Item { id: "app_maps", group: "apps", icon: "map", title_de: "Karten", title_en: "Maps",
        desc_de: "Offline-Karten-App.", desc_en: "Offline maps app.",
        tech: "Appx: Microsoft.WindowsMaps", warn_de: "", warn_en: "",
        presets: REC, actions: &[Action::Appx(&["Microsoft.WindowsMaps"])] },
    Item { id: "app_mail", group: "apps", icon: "mail", title_de: "Alte Mail-, Kalender- & Kontakte-App", title_en: "Old Mail, Calendar & People",
        desc_de: "Von Microsoft eingestellt.", desc_en: "Discontinued by Microsoft.",
        tech: "Appx: microsoft.windowscommunicationsapps, Microsoft.People", warn_de: "", warn_en: "",
        presets: REC, actions: &[Action::Appx(&["microsoft.windowscommunicationsapps", "Microsoft.People"])] },
    Item { id: "app_powerautomate", group: "apps", icon: "zap", title_de: "Power Automate", title_en: "Power Automate",
        desc_de: "Automatisierungs-Tool für Firmen.", desc_en: "Automation tool for businesses.",
        tech: "Appx: Microsoft.PowerAutomateDesktop", warn_de: "", warn_en: "",
        presets: REC, actions: &[Action::Appx(&["Microsoft.PowerAutomateDesktop"])] },
    Item { id: "app_family", group: "apps", icon: "users", title_de: "Microsoft Family", title_en: "Microsoft Family",
        desc_de: "Jugendschutz-App (Online-Dienst).", desc_en: "Parental controls app (online service).",
        tech: "Appx: MicrosoftCorporationII.MicrosoftFamily", warn_de: "", warn_en: "",
        presets: REC, actions: &[Action::Appx(&["MicrosoftCorporationII.MicrosoftFamily"])] },
    Item { id: "app_legacy3d", group: "apps", icon: "cube", title_de: "3D-Viewer, Paint 3D, Mixed Reality", title_en: "3D Viewer, Paint 3D, Mixed Reality",
        desc_de: "Nur bei älteren Builds vorhanden.", desc_en: "Only in older builds.",
        tech: "Appx: Microsoft.Microsoft3DViewer, Microsoft.MSPaint, Microsoft.MixedReality.Portal, Microsoft.SkypeApp, Microsoft.Wallet", warn_de: "", warn_en: "",
        presets: REC, actions: &[Action::Appx(&["Microsoft.Microsoft3DViewer", "Microsoft.MSPaint", "Microsoft.MixedReality.Portal", "Microsoft.SkypeApp", "Microsoft.Wallet"])] },
    Item { id: "app_phonelink", group: "apps", icon: "phone", title_de: "Smartphone-Link", title_en: "Phone Link",
        desc_de: "Verbindet Android/iPhone mit dem PC.", desc_en: "Connects Android/iPhone to the PC.",
        tech: "Appx: Microsoft.YourPhone, MicrosoftWindows.CrossDevice", warn_de: "Bluetooth selbst bleibt voll funktionsfähig.", warn_en: "Bluetooth itself keeps working.",
        presets: MAX, actions: &[Action::Appx(&["Microsoft.YourPhone", "MicrosoftWindows.CrossDevice"])] },
    Item { id: "app_quickassist", group: "apps", icon: "lifebuoy", title_de: "Remotehilfe (Quick Assist)", title_en: "Quick Assist",
        desc_de: "Fernwartung über Microsoft-Konto.", desc_en: "Remote help via Microsoft account.",
        tech: "Appx: MicrosoftCorporationII.QuickAssist", warn_de: "", warn_en: "",
        presets: MAX, actions: &[Action::Appx(&["MicrosoftCorporationII.QuickAssist"])] },
    Item { id: "app_media", group: "apps", icon: "music", title_de: "Medienwiedergabe & Filme/TV", title_en: "Media Player & Movies/TV",
        desc_de: "Standard-Player für Musik und Videos.", desc_en: "Default player for music and video.",
        tech: "Appx: Microsoft.ZuneMusic, Microsoft.ZuneVideo", warn_de: "Danach brauchst du einen eigenen Player (z. B. VLC).", warn_en: "You'll need your own player afterwards (e.g. VLC).",
        presets: MAX, actions: &[Action::Appx(&["Microsoft.ZuneMusic", "Microsoft.ZuneVideo"])] },
    Item { id: "app_recorder", group: "apps", icon: "mic", title_de: "Sprachrekorder", title_en: "Sound Recorder",
        desc_de: "Einfache Aufnahme-App.", desc_en: "Simple recording app.",
        tech: "Appx: Microsoft.WindowsSoundRecorder", warn_de: "", warn_en: "",
        presets: MAX, actions: &[Action::Appx(&["Microsoft.WindowsSoundRecorder"])] },
    Item { id: "app_sticky", group: "apps", icon: "note", title_de: "Kurznotizen", title_en: "Sticky Notes",
        desc_de: "Notizzettel-App.", desc_en: "Sticky notes app.",
        tech: "Appx: Microsoft.MicrosoftStickyNotes", warn_de: "", warn_en: "",
        presets: MAX, actions: &[Action::Appx(&["Microsoft.MicrosoftStickyNotes"])] },
    Item { id: "onedrive", group: "apps", icon: "cloud", title_de: "OneDrive abschalten", title_en: "Turn off OneDrive",
        desc_de: "OneDrive wird für neue Konten nicht mehr eingerichtet und die Dateisynchronisierung wird per Richtlinie gesperrt.",
        desc_en: "OneDrive is no longer set up for new accounts and file sync is blocked by policy.",
        tech: "Delete HKU\\DefaultUser\\…\\Run OneDriveSetup · Policies\\OneDrive DisableFileSyncNGSC=1",
        warn_de: "Wer OneDrive nutzt, sollte das aus lassen.", warn_en: "Leave this off if you use OneDrive.",
        presets: MAX,
        actions: &[
            Action::System(r#"RegSet 'HKLM\SOFTWARE\Policies\Microsoft\Windows\OneDrive' 'DisableFileSyncNGSC' 1"#),
            Action::DefaultUser(r#"reg.exe delete 'HKU\DefaultUser\Software\Microsoft\Windows\CurrentVersion\Run' /v 'OneDriveSetup' /f 2>$null | Out-Null"#),
        ],
    },
    Item { id: "legacy_caps", group: "apps", icon: "archive", title_de: "Alte Zusatzfunktionen", title_en: "Legacy extras",
        desc_de: "Schrittaufzeichnung, Mathematik-Eingabe und Internet-Explorer-Modus.", desc_en: "Steps Recorder, Math Input and Internet Explorer mode.",
        tech: "Remove-WindowsCapability: App.StepsRecorder, MathRecognizer, Browser.InternetExplorer", warn_de: "", warn_en: "",
        presets: MAX, actions: &[Action::Capability(&["App.StepsRecorder", "MathRecognizer", "Browser.InternetExplorer"])] },
    // ------------------------------------------------------------ setup
    Item {
        id: "offline_account", group: "setup", icon: "user",
        title_de: "Lokales Konto ohne Internet erlauben", title_en: "Allow local account without internet",
        desc_de: "Bei der Einrichtung erscheint „Ich habe kein Internet“, damit du ein lokales Konto ohne Microsoft-Konto anlegen kannst.",
        desc_en: "Setup shows “I don't have internet” so you can create a local account without a Microsoft account.",
        tech: "HKLM\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\OOBE BypassNRO=1",
        warn_de: "Funktioniert je nach Windows-Build; sonst während der Einrichtung einfach ohne Netzwerk starten.",
        warn_en: "Depends on the Windows build; otherwise just run setup without a network connection.",
        presets: REC,
        actions: &[Action::System(r#"RegSet 'HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\OOBE' 'BypassNRO' 1"#)],
    },
    Item {
        id: "bypass_hw", group: "setup", icon: "cpu",
        title_de: "Hardware-Prüfung umgehen (TPM, Secure Boot, CPU, RAM)", title_en: "Skip hardware check (TPM, Secure Boot, CPU, RAM)",
        desc_de: "Nur für ältere PCs nötig, die offiziell kein Windows 11 bekommen. Auf aktueller Hardware nicht anhaken.",
        desc_en: "Only needed for older PCs that officially can't run Windows 11. Leave it off on current hardware.",
        tech: "Windows PE: HKLM\\SYSTEM\\Setup\\LabConfig BypassTPMCheck, BypassSecureBootCheck, BypassCPUCheck, BypassRAMCheck, BypassStorageCheck = 1",
        warn_de: "Microsoft unterstützt solche PCs offiziell nicht.", warn_en: "Microsoft does not officially support such PCs.",
        presets: OFF,
        actions: &[Action::WinPe(&[
            r#"reg.exe add "HKLM\SYSTEM\Setup\LabConfig" /v BypassTPMCheck /t REG_DWORD /d 1 /f"#,
            r#"reg.exe add "HKLM\SYSTEM\Setup\LabConfig" /v BypassSecureBootCheck /t REG_DWORD /d 1 /f"#,
            r#"reg.exe add "HKLM\SYSTEM\Setup\LabConfig" /v BypassCPUCheck /t REG_DWORD /d 1 /f"#,
            r#"reg.exe add "HKLM\SYSTEM\Setup\LabConfig" /v BypassRAMCheck /t REG_DWORD /d 1 /f"#,
            r#"reg.exe add "HKLM\SYSTEM\Setup\LabConfig" /v BypassStorageCheck /t REG_DWORD /d 1 /f"#,
        ])],
    },
    Item {
        id: "no_autoencrypt", group: "setup", icon: "lock",
        title_de: "Automatische Laufwerksverschlüsselung aus", title_en: "No automatic drive encryption",
        desc_de: "Windows verschlüsselt das Systemlaufwerk nicht mehr ungefragt (BitLocker/Geräteverschlüsselung). Du kannst es später selbst einschalten.",
        desc_en: "Windows no longer encrypts the system drive without asking (BitLocker/device encryption). You can still turn it on yourself later.",
        tech: "HKLM\\SYSTEM\\CurrentControlSet\\Control\\BitLocker PreventDeviceEncryption=1",
        warn_de: "", warn_en: "",
        presets: OFF,
        actions: &[Action::System(r#"RegSet 'HKLM\SYSTEM\CurrentControlSet\Control\BitLocker' 'PreventDeviceEncryption' 1"#)],
    },
    // ------------------------------------------------------------ comfort
    Item {
        id: "fileext", group: "comfort", icon: "file",
        title_de: "Dateiendungen anzeigen", title_en: "Show file extensions",
        desc_de: "Zeigt im Explorer .exe, .txt usw. an – sicherer gegen getarnte Schadsoftware.",
        desc_en: "Shows .exe, .txt etc. in Explorer – safer against disguised malware.",
        tech: "HKCU Explorer\\Advanced HideFileExt=0", warn_de: "", warn_en: "",
        presets: OFF,
        actions: &[Action::DefaultUser(r#"RegSet 'HKU\DefaultUser\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced' 'HideFileExt' 0"#)],
    },
    Item {
        id: "classicmenu", group: "comfort", icon: "menu",
        title_de: "Klassisches Rechtsklick-Menü", title_en: "Classic right-click menu",
        desc_de: "Das vollständige Kontextmenü ohne „Weitere Optionen anzeigen“.",
        desc_en: "The full context menu without “Show more options”.",
        tech: "HKCU Software\\Classes\\CLSID\\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}\\InprocServer32 (Default)=\"\"", warn_de: "", warn_en: "",
        presets: OFF,
        actions: &[Action::DefaultUser(r#"$k = 'Registry::HKEY_USERS\DefaultUser\Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}\InprocServer32'
New-Item -Path $k -Force | Out-Null
Set-ItemProperty -LiteralPath $k -Name '(default)' -Value ''"#)],
    },
    Item {
        id: "taskbarleft", group: "comfort", icon: "align-left",
        title_de: "Startmenü links", title_en: "Start menu on the left",
        desc_de: "Taskleisten-Symbole links ausrichten wie bei Windows 10.", desc_en: "Align taskbar icons to the left like Windows 10.",
        tech: "HKCU Explorer\\Advanced TaskbarAl=0", warn_de: "", warn_en: "",
        presets: OFF,
        actions: &[Action::DefaultUser(r#"RegSet 'HKU\DefaultUser\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced' 'TaskbarAl' 0"#)],
    },
    Item {
        id: "nofaststartup", group: "comfort", icon: "power",
        title_de: "Schnellstart aus (Dual-Boot)", title_en: "Fast startup off (dual boot)",
        desc_de: "Empfohlen, wenn du neben Windows auch SteamOS oder Linux nutzt: Windows sperrt dann beim Herunterfahren keine Laufwerke mehr.",
        desc_en: "Recommended if you also use SteamOS or Linux: Windows no longer locks drives on shutdown.",
        tech: "HKLM\\SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Power HiberbootEnabled=0", warn_de: "", warn_en: "",
        presets: OFF,
        actions: &[Action::System(r#"RegSet 'HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Power' 'HiberbootEnabled' 0"#)],
    },
];

pub fn preset_ids(p: Preset) -> Vec<&'static str> {
    let idx = match p {
        Preset::Gentle => 0,
        Preset::Recommended => 1,
        Preset::Maximum => 2,
    };
    ITEMS.iter().filter(|i| i.presets[idx]).map(|i| i.id).collect()
}

fn js(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn js_list(v: &[&str]) -> String {
    format!("[{}]", v.iter().map(|s| js(s)).collect::<Vec<_>>().join(","))
}

/// Catalog as JSON for the UI (no serde needed).
pub fn catalog_json() -> String {
    let groups: Vec<String> = GROUPS
        .iter()
        .map(|g| {
            format!(
                "{{\"id\":{},\"icon\":{},\"title_de\":{},\"title_en\":{},\"desc_de\":{},\"desc_en\":{}}}",
                js(g.id), js(g.icon), js(g.title_de), js(g.title_en), js(g.desc_de), js(g.desc_en)
            )
        })
        .collect();
    let items: Vec<String> = ITEMS
        .iter()
        .map(|i| {
            format!(
                "{{\"id\":{},\"group\":{},\"icon\":{},\"title_de\":{},\"title_en\":{},\"desc_de\":{},\"desc_en\":{},\"tech\":{},\"warn_de\":{},\"warn_en\":{},\"presets\":[{},{},{}]}}",
                js(i.id), js(i.group), js(i.icon), js(i.title_de), js(i.title_en), js(i.desc_de), js(i.desc_en),
                js(i.tech), js(i.warn_de), js(i.warn_en), i.presets[0], i.presets[1], i.presets[2]
            )
        })
        .collect();
    format!(
        "{{\"groups\":[{}],\"items\":[{}],\"protected_de\":{},\"protected_en\":{}}}",
        groups.join(","),
        items.join(","),
        js_list(PROTECTED_LABELS_DE),
        js_list(PROTECTED_LABELS_EN)
    )
}
