import { getLanguageState, translate, type TranslationKey } from "$lib/i18n";

export type EggDoneErrorCode =
  | "SYNC_CREDENTIALS"
  | "SYNC_NETWORK"
  | "SYNC_FAILED"
  | "DATA_EXCHANGE_FAILED"
  | "ATTACHMENT_FAILED"
  | "REMINDER_FAILED"
  | "FOCUS_UNAVAILABLE";

const PREFIX = "EGGDONE_ERROR::";
const MESSAGE_KEYS: Record<EggDoneErrorCode, [TranslationKey, TranslationKey]> = {
  SYNC_CREDENTIALS: ["error.syncCredentials", "error.syncCredentialsAction"],
  SYNC_NETWORK: ["error.syncNetworkTitle", "error.syncNetworkAction"],
  SYNC_FAILED: ["error.syncFailed", "error.syncFailedAction"],
  DATA_EXCHANGE_FAILED: ["error.dataExchange", "error.dataExchangeAction"],
  ATTACHMENT_FAILED: ["error.attachment", "error.attachmentAction"],
  REMINDER_FAILED: ["error.reminder", "error.reminderAction"],
  FOCUS_UNAVAILABLE: ["error.focus", "error.focusAction"],
};

export function codedInvoke<T>(promise: Promise<T>, code: EggDoneErrorCode): Promise<T> {
  return promise.catch((reason: unknown) => Promise.reject(ensureErrorCode(reason, code)));
}

export function ensureErrorCode(reason: unknown, code: EggDoneErrorCode): string {
  const raw = rawError(reason);
  return raw.startsWith(PREFIX) ? raw : `${PREFIX}${code}::${safeDetail(raw)}`;
}

export function localizedErrorMessage(reason: unknown): string {
  const raw = rawError(reason);
  const progressKey = progressErrorKey(raw);
  if (progressKey) return translate(getLanguageState().resolvedLocale, progressKey);
  if (raw.includes('SYNC_AUTO_JOIN_')) return translate(getLanguageState().resolvedLocale, 'sync.autoJoinFailed');
  const parsed = parseCodedError(raw);
  if (!parsed) return safeDetail(raw);
  const locale = getLanguageState().resolvedLocale;
  if (parsed.code === "DATA_EXCHANGE_FAILED") {
    if (/RECURRENCE_SYNC_BUSY|同步正在进行/.test(parsed.detail)) return translate(locale, "data.restoreBusy");
    if (/RECURRENCE_OCCURRENCE_MISSING/.test(parsed.detail)) return translate(locale, "data.restorePurged");
    if (/TASK_NOTE_LINK/.test(parsed.detail)) return translate(locale, "data.restoreLinksFailed");
    if (/RECURRENCE/.test(parsed.detail)) return translate(locale, "data.restoreRuleConflict");
  }
  const [titleKey, actionKey] = MESSAGE_KEYS[parsed.code];
  return `${translate(locale, titleKey)} ${translate(locale, actionKey)}`;
}

export function progressErrorKey(raw: string): TranslationKey | null {
  const code = raw.match(/\bPROGRESS_[A-Z_]+\b/)?.[0];
  if (!code) return null;
  const keys: Record<string, TranslationKey> = {
    PROGRESS_INVALID: 'taskProgress.sync.invalid', PROGRESS_LIMIT: 'taskProgress.error.limit',
    PROGRESS_CONFLICT: 'taskProgress.sync.conflict', PROGRESS_SYNC_CONFLICT: 'taskProgress.sync.conflict',
    PROGRESS_ETAG_REQUIRED: 'taskProgress.sync.conflict', PROGRESS_NOT_EMPTY: 'taskProgress.sync.conflict',
    PROGRESS_REMOTE_MISSING: 'taskProgress.sync.missing', PROGRESS_CONFIG_CHANGED: 'taskProgress.sync.target',
    PROGRESS_KEY_INVALID: 'taskProgress.sync.key', PROGRESS_KEY_COLLISION: 'taskProgress.sync.key',
    PROGRESS_DATABASE: 'taskProgress.sync.database', PROGRESS_NETWORK: 'taskProgress.sync.network',
    PROGRESS_READ_ONLY: 'taskProgress.error.readOnly', PROGRESS_UNAVAILABLE: 'taskProgress.error.unavailable',
    PROGRESS_DELETED: 'taskProgress.error.deleted',
  };
  if (code === 'PROGRESS_HTTP') return /PROGRESS_HTTP:(401|403)\b/.test(raw) ? 'taskProgress.sync.denied' : 'taskProgress.sync.network';
  return keys[code] ?? 'taskProgress.sync.failed';
}

function parseCodedError(raw: string): { code: EggDoneErrorCode; detail: string } | null {
  if (!raw.startsWith(PREFIX)) return null;
  const separator = raw.indexOf("::", PREFIX.length);
  const code = (separator < 0 ? raw.slice(PREFIX.length) : raw.slice(PREFIX.length, separator)) as EggDoneErrorCode;
  if (!(code in MESSAGE_KEYS)) return null;
  return { code, detail: separator < 0 ? "" : raw.slice(separator + 2) };
}

function rawError(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason ?? "");
}

function safeDetail(raw: string): string {
  const firstLine = raw.split(/\r?\n/, 1)[0].trim();
  return (firstLine || "Unknown error").slice(0, 220);
}
