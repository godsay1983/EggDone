export interface BatchItem { uuid: string; title: string }
export interface BatchRequest { operation_uuid: string; group_uuid: string | null; items: BatchItem[] }
export interface BatchResult { operation_uuid: string; task_uuids: string[]; created_at: number }
