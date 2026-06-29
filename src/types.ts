export type Flavor = "Cloud" | "Server";

export interface IssueSummary {
  key: string;
  summary: string;
  points: number;
  resolved: string; // ISO 8601
}

export interface Snapshot {
  total_points: number;
  issue_count: number;
  issues: IssueSummary[];
  fetched_at: string; // ISO 8601
}

export interface FieldChoice {
  id: string;
  name: string;
}

export type SetupResult =
  | { kind: "ok" }
  | { kind: "needs_field_pick"; candidates: FieldChoice[] };

export interface JiraError {
  kind:
    | "Network"
    | "Auth"
    | "NoStoryPointsField"
    | "Keychain"
    | "Io"
    | "Parse";
  message: string;
  candidates?: FieldChoice[];
}

export interface RefreshFailed {
  reason: string;
}
