# Custom Entity Types for the "Story" Graph

This document defines custom entity types for representing the complex knowledge graph of the "Vice City: Pantelimon" story universe. Each type includes recommended properties with their data types.

## 1. Character

The core entity representing a person/character in the story.

### Properties

| Property | Type | Description | Source |
|----------|------|-------------|--------|
| `name` | string | Full name of the character | Required |
| `nickname` | string | Informal name/pseudonym | Optional |
| `role` | string | Primary role in story (e.g., "Protagonist", "Antagonist", "Boss") | Optional |
| `description` | string | Brief character summary | Optional |
| `biography` | string | Wikipedia-style background story | Optional |
| `traits` | array of strings | Personality traits, quirks, habits | Optional |
| `age` | string or integer | Age or age range (e.g., "teenage", "elderly") | Optional |
| `status` | string | Current status (e.g., "retired", "active", "deceased") | Optional |
| `occupation` | string | Legitimate job or cover | Optional |
| `interests` | array of strings | Hobbies, preferences | Optional |
| `vehicle` | string | Primary vehicle description | Optional |
| `gym` | string | Training location | Optional |
| `fighting_style` | string | Preferred combat method | Optional |
| `source_file` | string | Which markdown file this came from | Optional |
| `source_story` | string | "main" or "alternate_story" | Optional |

### Example
```json
{
  "name": "Relu Oncescu",
  "type": "Character",
  "nickname": null,
  "role": "Protagonist / Taximetrist / Recuperator",
  "description": "Main protagonist. Appears to be an ordinary Bucharest taxi driver but secretly works as a debt collector for Căpitanu'.",
  "biography": "Născut și crescut în Pantelimon...",
  "traits": ["extremely calculating", "quiet", "violent when necessary"],
  "vehicle": "yellow Dacia Logan 1.4 MPI on GPL",
  "gym": "hidden boxing gym in a Pantelimon block basement",
  "fighting_style": "prefers fists/bar fights over firearms"
}
```

---

## 2. Place

A geographical location where events occur.

### Properties

| Property | Type | Description | Source |
|----------|------|-------------|--------|
| `name` | string | Location name (city, district, specific venue) | Required |
| `type` | string | Subtype: "city", "sector", "neighborhood", "venue", "port" | Optional |
| `description` | string | Brief description of the place | Optional |
| `significance` | string | Why it matters to the story | Optional |
| `related_characters` | array of strings | Characters associated with this place | Optional |

### Example
```json
{
  "name": "Pantelimon",
  "type": "Place",
  "description": "Neighborhood in eastern Bucharest, sector 2",
  "significance": "Primary setting for most criminal activities"
}
```

---

## 3. Organization

A formal or informal group/organization (criminal clan, mafia, police unit).

### Properties

| Property | Type | Description | Source |
|----------|------|-------------|--------|
| `name` | string | Organization name or identifier | Required |
| `type` | string | "criminal_clan", "mafia_family", "police_unit", "business" | Optional |
| `description` | string | What the organization does | Optional |
| `leader` | string | Name of the leader character | Optional |
| `territory` | string | Geographic area of control | Optional |
| `activities` | array of strings | Illegal/legal activities conducted | Optional |

### Example
```json
{
  "name": "Căpitanu's Clan",
  "type": "Organization",
  "type_subcategory": "criminal_clan",
  "description": "Criminal clan controlling illegal activities in Sector 2",
  "leader": "Căpitanu'",
  "territory": "Pantelimon, Sector 2",
  "activities": ["prostitution", "usury", "protection rackets"]
}
```

---

## 4. Vehicle

A vehicle owned or used by a character.

### Properties

| Property | Type | Description |
|----------|------|-------------|
| `name` | string | Vehicle identifier (make/model or nickname) |
| `type` | string | "car", "truck", "motorcycle", etc. |
| `owner` | string | Character who owns/uses it |
| `description` | string | Color, model, modifications, condition |
| `significance` | string | Story significance (e.g., "primary work vehicle") |

---

## 5. Establishment

A physical location that serves as a business or frequently-visited spot.

### Properties

| Property | Type | Description |
|----------|------|-------------|
| `name` | string | Name of the establishment |
| `type` | string | "restaurant", "shop", "gym", "bar", "cafe", "club" |
| `location` | string | Where it's located (place name or address) |
| `owner` | string | Who owns/operates it |
| `description` | string | What it's like, atmosphere |
| `associated_characters` | array of strings | Characters who frequent/work there |

---

## 6. Weapon

A weapon used by a character.

### Properties

| Property | Type | Description |
|----------|------|-------------|
| `name` | string | Weapon name/type |
| `type` | string | "firearm", "melee", "improvised" |
| `owner` | string | Character who possesses it |
| `description` | string | Model, condition, modifications |
| `status` | string | "functional", "collector's", "never used", etc. |

---

## 7. Item

An object of significance in the story.

### Properties

| Property | Type | Description |
|----------|------|-------------|
| `name` | string | Item name |
| `category` | string | "food", "clothing", "parfum", "music", "tool" |
| `owner` | string | Character who owns/uses it |
| `description` | string | What it is and why it matters |
| `cultural_significance` | string | Romanian/Easter egg context if applicable |

---

## 8. Event / Plot Point

A specific occurrence or story beat.

### Properties

| Property | Type | Description |
|----------|------|-------------|
| `name` | string | Event title/summary |
| `type` | string | "conflict", "discovery", "death", "arrest", "business_deal" |
| `season` | integer | Which season it occurs in |
| `episode` | integer | Episode number |
| `participants` | array of strings | Characters involved |
| `location` | string | Where it happened |
| `outcome` | string | Result/consequences |
| `trigger_event` | string | What caused it |

---

## Notes on Typing Strategy

Graphiti's schema-less nature means entity types emerge from usage. To maintain consistency:

1. **Use consistent `type` labels**: Always label nodes as `Character`, `Place`, `Organization`, `Vehicle`, etc.
2. **Standardize property names**: Use the property names defined here exactly.
3. **Reference by name**: Relationships should reference characters by their `name` field, not UUID.
4. **Enrich gradually**: Add new properties as discovered in future episodes.
5. **Source tracking**: Include `source_file` and `source_story` to track origin of information.
