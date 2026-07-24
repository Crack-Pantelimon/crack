# Custom Edge (Relationship) Types for the "Story" Graph

This document defines custom edge types for representing relationships between entities in the "Vice City: Pantelimon" story universe. Each edge type includes recommended properties for capturing nuanced connections.

## 1. Family

Kinship relationships between characters.

### Properties

| Property | Type | Description | Values |
|----------|------|-------------|--------|
| `relation_type` | string | Specific family relationship | "parent", "child", "spouse", "sibling", "uncle", "aunt", "nephew", "niece", "cousin", "father", "mother", "son", "daughter", "brother", "sister", "in_law" |
| `strength` | string | Quality of the relationship | "close", "strained", "estranged", "complex", "neutral" (default) |
| `cohabitation` | boolean | Do they live together? | Optional |
| `financial_support` | boolean | Does one support the other financially? | Optional |

### Example
```json
{
  "source": "Relu Oncescu",
  "target": "Gina Oncescu",
  "type": "Family",
  "relation_type": "spouse",
  "strength": "complex"
}
```

---

## 2. Employment

Formal or informal employment/subordinate relationships.

### Properties

| Property | Type | Description |
|----------|------|-------------|
| `role` | string | Job title/role (e.g., "enforcer", "assistant", "accountant") |
| `organization` | string | Which organization/entity employs (if applicable) |
| `salary` | string or number | Compensation (if known) |
| `loyalty` | string | "high", "medium", "low", "questionable", "blackmailed" |
| `direct_reports` | array of strings | Who reports to this person (optional reverse edge) |

### Example
```json
{
  "source": "Căpitanu'",
  "target": "Relu Oncescu",
  "type": "Employment",
  "role": "principal enforcer",
  "organization": "Căpitanu's Clan",
  "loyalty": "high"
}
```

---

## 3. Mentorship

A character teaching/guiding another.

### Properties

| Property | Type | Description |
|----------|------|-------------|
| `context` | string | Why the mentorship exists (e.g., "apprenticeship", "initiation") |
| `duration` | string | "ongoing", "temporary", "forced" |
| `effectiveness` | string | "successful", "struggling", "abusive" |
| `field` | string | What is being taught (e.g., "criminal activities", "street smarts") |

### Example
```json
{
  "source": "Relu Oncescu",
  "target": "Teddy",
  "type": "Mentorship",
  "context": "apprenticeship for mobster training",
  "duration": "ongoing",
  "effectiveness": "struggling"
}
```

---

## 4. Romantic

Romantic attraction, relationship, or marriage.

### Properties

| Property | Type | Description | Values |
|----------|------|-------------|--------|
| `relationship_status` | string | Current state | "attraction", "dating", "married", "separated", "ex", "secret" |
| `seriousness` | string | Level of commitment | "casual", "serious", "life_partners" |
| `approved_by_families` | boolean | Do families support the relationship? | Optional |
| `consequences` | string | Impact on story (e.g., "complicated alliances") | Optional |

### Example
```json
{
  "source": "Teddy",
  "target": "Magda Oncescu",
  "type": "Romantic",
  "relationship_status": "dating",
  "seriousness": "serious",
  "approved_by_families": false,
  "consequences": "Complicates clan alliances"
}
```

---

## 5. Hostility

Negative relationships involving opposition, threat, or coercion.

### Properties

| Property | Type | Description | Values |
|----------|------|-------------|--------|
| `subtype` | string | Type of hostility | "enemy", "target", "rival", "blackmail", "threat", "harassment", "abuse" |
| `initiator` | string | Who started the hostility | Optional |
| `motivation` | string | Why the hostility exists | Optional |
| `intensity` | string | "low", "moderate", "high", "lethal" |
| `current_status` | string | "active", "dormant", "resolved", "escalating" |

### Example
```json
{
  "source": "Emilian",
  "target": "Relu Oncescu",
  "type": "Hostility",
  "subtype": "enemy",
  "motivation": "professional duty / personal obsession",
  "intensity": "high",
  "current_status": "active"
}
```

---

## 6. Suspicion

Character believes another is involved in something shady.

### Properties

| Property | Type | Description |
|----------|------|-------------|
| `basis` | string | What caused suspicion (e.g., "observed behavior", "rumors") |
| `certainty` | string | "suspicious", "convinced", "investigating" |
| `actions_taken` | array of strings | What they've done about it (e.g., "spied", "confronted") |
| `knowledge_level` | string | How much they actually know | "none", "partial", "full" |

### Example
```json
{
  "source": "Sabin",
  "target": "Relu Oncescu",
  "type": "Suspicion",
  "basis": "observed odd behavior, smells, schedule",
  "certainty": "convinced",
  "actions_taken": ["spying", "confrontation attempts"],
  "knowledge_level": "partial"
}
```

---

## 7. Acquaintance

Looser social or professional connections.

### Properties

| Property | Type | Description |
|----------|------|-------------|
| `context` | string | Where/how they know each other (e.g., "through Relu", "neighborhood") |
| `frequency` | string | "daily", "weekly", "occasionally", "rarely" |
| `trust_level` | string | "none", "low", "moderate", "high" |
| `favors_exchanged` | boolean | Have they done favors for each other? |

### Example
```json
{
  "source": "Haralambie Olaru",
  "target": "Nea Puiu",
  "type": "Acquaintance",
  "context": "neighborhood - Hari patrols, Nea Puiu hangs at gym",
  "frequency": "occasionally",
  "trust_level": "moderate",
  "favors_exchanged": true
}
```

---

## 8. Business Partnership

Formal or informal business collaboration.

### Properties

| Property | Type | Description | Values |
|----------|------|-------------|--------|
| `venture_type` | string | What business | "money_laundering", "drug_distribution", "prostitution", "usury", "protection", "legitimate_business" |
| `equity_split` | string | How profits/control divided | Optional |
| `trust_level` | string | "complete", "tense", "suspicious", "backstabbing" |
| `secrecy` | string | How hidden the partnership is | "open", "secret", "plausible_deniability" |
| `territory` | string | Geographic area of operation | Optional |

### Example
```json
{
  "source": "Toma",
  "target": "Căpitanu'",
  "type": "BusinessPartnership",
  "venture_type": ["money_laundering", "drug_distribution"],
  "trust_level": "tense",
  "secrecy": "secret",
  "territory": "Bucharest-Constanța corridor"
}
```

---

## 9. Territorial Control

Which character or organization controls/influences a place.

### Properties

| Property | Type | Description |
|----------|------|-------------|
| `control_level` | string | "full", "partial", "contested", "influence" |
| `activities_conducted` | array of strings | What illegal activities happen there |
| `enforcement` | string | How control is maintained (e.g., "protection_racket", "police_presence") |
| `conflicts` | string | Any disputes over this territory | Optional |

### Example
```json
{
  "source": "Căpitanu'",
  "target": "Pantelimon",
  "type": "TerritorialControl",
  "control_level": "full",
  "activities_conducted": ["prostitution", "usury", "protection rackets"],
  "enforcement": "through Relu and Nico"
}
```

---

## 10. Affiliation

Membership or association with an organization.

### Properties

| Property | Type | Description |
|----------|------|-------------|
| `organization` | string | Name of the organization |
| `rank` | string | Position/rank within organization | 
| `initiation_date` | string | When they joined | Optional |
| `status` | string | "active", "inactive", "suspended", "expelled" |
| `allegiance` | string | How loyal they are | "total", "conditional", "questionable" |

### Example
```json
{
  "source": "Relu Oncescu",
  "target": "Căpitanu's Clan",
  "type": "Affiliation",
  "organization": "Căpitanu's Clan",
  "rank": "principal enforcer",
  "status": "active",
  "allegiance": "total"
}
```

---

## 11. Possession

Character owns or possesses an item/vehicle/weapon.

### Properties

| Property | Type | Description |
|----------|------|-------------|
| `item_type` | string | "vehicle", "weapon", "item", "property" |
| `value` | string | Approximate value or importance | Optional |
| `legality` | string | "legal", "illegal", "gray_area" |
| `concealed` | boolean | Is it hidden/illegal to possess? | Optional |
| `used_for` | array of strings | Purposes (e.g., "work", "protection", "status") |

### Example
```json
{
  "source": "Relu Oncescu",
  "target": "Dacia Logan",
  "type": "Possession",
  "item_type": "vehicle",
  "description": "yellow Dacia Logan 1.4 MPI on GPL",
  "value": "low",
  "legality": "legal",
  "used_for": ["taxi work", "recovery missions"]
}
```

---

## 12. Frequent

Character regularly visits an establishment.

### Properties

| Property | Type | Description |
|----------|------|-------------|
| `frequency` | string | "daily", "weekly", "occasionally" |
| `purpose` | string | Why they go there (e.g., "earn bribes", "coffee", "meet contacts") |
| `duration` | string | How long they've been going there | Optional |
| `social_standing` | string | How they're treated there | Optional |

### Example
```json
{
  "source": "Haralambie Olaru",
  "target": "Corner Birt (local bar)",
  "type": "Frequent",
  "frequency": "daily",
  "purpose": "collect tips (covrigi, coffee), avoid work",
  "duration": "years"
}
```

---

## 13. Annoyance

One character irritates another (weaker negative connection).

### Properties

| Property | Type | Description |
|----------|------|-------------|
| `reason` | string | Why they're annoyed |
| `tolerance_level` | string | "low", "moderate", "high", "at_wits_end" |
| `response` | string | How they deal with it (e.g., "ignores", "shouts", "violence") |

### Example
```json
{
  "source": "Nea Puiu",
  "target": "Gina Oncescu",
  "type": "Annoyance",
  "reason": "she is nagging/controlling",
  "tolerance_level": "low",
  "response": "avoids when possible, gets angry"
}
```

---

## Summary Table of Edge Types

| Edge Type | Primary Use | Key Properties |
|-----------|-------------|----------------|
| Family | Kinship | `relation_type`, `strength` |
| Employment | Boss/worker ties | `role`, `organization`, `loyalty` |
| Mentorship | Teaching/guidance | `context`, `duration`, `effectiveness` |
| Romantic | Love/marriage | `relationship_status`, `seriousness` |
| Hostility | Enemies/rivals | `subtype`, `intensity`, `current_status` |
| Suspicion | Distrust/surveillance | `basis`, `certainty`, `knowledge_level` |
| Acquaintance | Loose connections | `context`, `trust_level` |
| BusinessPartnership | Criminal/business collusion | `venture_type`, `trust_level`, `secrecy` |
| TerritorialControl | Place control | `control_level`, `activities_conducted` |
| Affiliation | Organization membership | `organization`, `rank`, `allegiance` |
| Possession | Ownership of things | `item_type`, `value`, `legality` |
| Frequent | Regular visits | `frequency`, `purpose` |
| Annoyance | Minor irritation | `reason`, `tolerance_level` |

---

## Implementation Notes

1. **Bidirectional relationships**: Some edges are directional (e.g., Employment, Mentorship), others bidirectional (Family, Romantic). Store both directions if needed for queries.
2. **Edge naming in Graphiti**: Use `type` field exactly as shown above (CamelCase preferred).
3. **Multiple subtypes**: For edges like Hostility, the `subtype` captures the specific flavor (enemy, rival, etc.).
4. **Temporal aspects**: For relationships that change over time, consider adding `start_date` and `end_date` properties.
5. **Temporal validity**: Graphiti supports `valid_at` and `expired_at` timestamps for time-aware edges.
