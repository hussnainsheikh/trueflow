export type OnboardingState = "first_run" | "in_progress" | "ready";
export type OnboardingStep =
  | "welcome"
  | "microphone"
  | "accessibility"
  | "account"
  | "shortcut"
  | "first_dictation"
  | "behavior"
  | "complete";

export type ProductSettings = {
  onboardingState: OnboardingState;
  onboardingStep: OnboardingStep;
  historyEnabled: boolean;
  analyticsEnabled: boolean;
  showOverlay: boolean;
  dictationShortcut: string;
  rewriteShortcut: string;
  dictationPaused: boolean;
  microphoneDeviceId: string | null;
  microphoneDeviceName: string | null;
};

export type InputDevice = {
  id: string;
  name: string;
  isDefault: boolean;
};

export type MicrophoneOptions = {
  devices: InputDevice[];
  selectedId: string | null;
  selectedName: string | null;
  selectedAvailable: boolean;
};

export type HistoryItem = {
  id: string;
  kind: string;
  projectName: string | null;
  voiceName: string | null;
  modeName: string | null;
  sourceApp: string | null;
  finalText: string;
  truncated: boolean;
  status: string;
  errorCode: string | null;
  createdAt: string;
};

export type HistoryPage = {
  items: HistoryItem[];
  total: number;
  hasMore: boolean;
};
export type AccountStatus = {
  state: "signedOut" | "signedIn" | "sessionExpired" | "offline";
  configured: boolean;
  signedIn: boolean;
  email: string | null;
  provider: string | null;
  offline: boolean;
  cloudReady: boolean;
};
export type Diagnostics = {
  applicationVersion: string;
  os: string;
  architecture: string;
  environment: string;
  schemaVersion: number;
  transcriptionProvider: string;
  transformationProvider: string;
  authenticationConfigured: boolean;
  analyticsConfigured: boolean;
  crashReportingConfigured: boolean;
  microphonePermission: string;
  accessibilityPermission: string;
  lastErrorCategory: string | null;
};
export type AuthStart = {
  verificationUrl: string;
  userCode: string;
  expiresInSeconds: number;
};

export function parseProductSettings(value: unknown): ProductSettings {
  if (
    !record(value) ||
    !["first_run", "in_progress", "ready"].includes(
      String(value.onboardingState),
    ) ||
    ![
      "welcome",
      "microphone",
      "accessibility",
      "account",
      "shortcut",
      "first_dictation",
      "behavior",
      "complete",
    ].includes(String(value.onboardingStep)) ||
    typeof value.historyEnabled !== "boolean" ||
    typeof value.analyticsEnabled !== "boolean" ||
    typeof value.showOverlay !== "boolean" ||
    typeof value.dictationShortcut !== "string" ||
    typeof value.rewriteShortcut !== "string" ||
    typeof value.dictationPaused !== "boolean" ||
    !nullableString(value.microphoneDeviceId) ||
    !nullableString(value.microphoneDeviceName)
  )
    throw new Error("Invalid product settings payload");
  return value as ProductSettings;
}

export function parseHistoryPage(value: unknown): HistoryPage {
  if (
    !record(value) ||
    !Array.isArray(value.items) ||
    typeof value.total !== "number" ||
    typeof value.hasMore !== "boolean" ||
    !value.items.every(isHistoryItem)
  )
    throw new Error("Invalid history payload");
  return value as HistoryPage;
}

export function parseAccountStatus(value: unknown): AccountStatus {
  if (
    !record(value) ||
    !["signedOut", "signedIn", "sessionExpired", "offline"].includes(
      String(value.state),
    ) ||
    typeof value.configured !== "boolean" ||
    typeof value.signedIn !== "boolean" ||
    !(value.email === null || typeof value.email === "string") ||
    !(value.provider === null || typeof value.provider === "string") ||
    typeof value.offline !== "boolean" ||
    typeof value.cloudReady !== "boolean"
  )
    throw new Error("Invalid account payload");
  return value as AccountStatus;
}

export function parseDiagnostics(value: unknown): Diagnostics {
  if (
    !record(value) ||
    typeof value.applicationVersion !== "string" ||
    typeof value.os !== "string" ||
    typeof value.architecture !== "string" ||
    typeof value.environment !== "string" ||
    typeof value.schemaVersion !== "number" ||
    typeof value.transcriptionProvider !== "string" ||
    typeof value.transformationProvider !== "string" ||
    typeof value.authenticationConfigured !== "boolean" ||
    typeof value.analyticsConfigured !== "boolean" ||
    typeof value.crashReportingConfigured !== "boolean" ||
    typeof value.microphonePermission !== "string" ||
    typeof value.accessibilityPermission !== "string" ||
    !nullableString(value.lastErrorCategory)
  )
    throw new Error("Invalid diagnostics payload");
  return value as Diagnostics;
}

export function parseAuthStart(value: unknown): AuthStart {
  if (
    !record(value) ||
    typeof value.verificationUrl !== "string" ||
    typeof value.userCode !== "string" ||
    typeof value.expiresInSeconds !== "number"
  )
    throw new Error("Invalid authentication payload");
  return value as AuthStart;
}

export function parseMicrophoneOptions(value: unknown): MicrophoneOptions {
  if (
    !record(value) ||
    !Array.isArray(value.devices) ||
    !value.devices.every(isInputDevice) ||
    !nullableString(value.selectedId) ||
    !nullableString(value.selectedName) ||
    typeof value.selectedAvailable !== "boolean"
  )
    throw new Error("Invalid microphone payload");
  return value as MicrophoneOptions;
}

function isInputDevice(value: unknown): value is InputDevice {
  return (
    record(value) &&
    typeof value.id === "string" &&
    typeof value.name === "string" &&
    typeof value.isDefault === "boolean"
  );
}

function isHistoryItem(value: unknown): value is HistoryItem {
  return (
    record(value) &&
    typeof value.id === "string" &&
    typeof value.kind === "string" &&
    nullableString(value.projectName) &&
    nullableString(value.voiceName) &&
    nullableString(value.modeName) &&
    nullableString(value.sourceApp) &&
    typeof value.finalText === "string" &&
    typeof value.truncated === "boolean" &&
    typeof value.status === "string" &&
    nullableString(value.errorCode) &&
    typeof value.createdAt === "string"
  );
}
function nullableString(value: unknown): boolean {
  return value === null || typeof value === "string";
}
function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
