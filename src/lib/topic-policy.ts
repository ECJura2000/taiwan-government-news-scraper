export interface NegativeRule { text: string; enabled: boolean; penalty: number; match_fields: string[] }
export interface WeightedKeyword { text: string; weight: number; enabled: boolean; origin: string; references: Record<string, unknown>[] }
export interface Topic {
  name: string; enabled: boolean; lead_agency: string; lead_source: string;
  exact_phrases: string[]; strong_keywords: string[]; context_keywords: string[];
  weighted_keywords: WeightedKeyword[]; penalty_keywords: NegativeRule[]; exclude_keywords: NegativeRule[];
}
export interface TopicPolicy {
  schema_version: number; version: string; name: string; initiatives: Topic[];
  general_keywords: string[]; thresholds: { high: number; possible: number; negative_penalty: number };
  scoring: { k1: number; b: number; title_weight: number; summary_weight: number; general_weight: number };
  references: Record<string, unknown>[];
}
export interface ImportPreview {
  profile: TopicPolicy; added: string[]; updated: string[]; deleted: string[];
  common_changes: { field: string; before: unknown; after: unknown }[];
}
export function enabledCount(profile: TopicPolicy | null): number { return profile?.initiatives.filter(t => t.enabled).length ?? 0; }
export function clonePolicy<T>(value: T): T { return JSON.parse(JSON.stringify(value)) as T; }
export const ruleFields = (rule: NegativeRule, field: string, checked: boolean): string[] => checked
  ? [...new Set([...rule.match_fields, field])]
  : rule.match_fields.filter(f => f !== field);
