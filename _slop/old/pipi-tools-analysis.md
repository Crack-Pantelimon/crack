# Pi Tool Analysis

This document provides a detailed analysis of the tools available to the coding agent.

## 1. `read`
Reads the contents of a file. Supports text files and images.

### Arguments
| Argument | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `path` | `STRING` | Yes | Path to the file to read (relative or absolute). |
| `offset` | `NUMBER` | No | Line number to start reading from (1-indexed). Used for large files. |
| `limit` | `NUMBER` | No | Maximum number of lines to read. Output is truncated to 2000 lines or 50KB if not specified. |

### Behavioral Constraints
- Images are sent as attachments.
- For text files, if truncated, the agent should continue using `offset` until the full file is read.

---

## 2. `bash`
Executes a bash command in the current working directory.

### Arguments
| Argument | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `command` | `STRING` | Yes | Bash command to execute. |
| `timeout` | `NUMBER` | No | Timeout in seconds (optional, no default timeout). |

### Behavioral Constraints
- Returns `stdout` and `stderr`.
- Output is truncated to the last 2000 lines or 50KB.
- If truncated, the full output is saved to a temporary file.

---

## 3. `edit`
Edits a single file using exact text replacement.

### Arguments
| Argument | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `path` | `STRING` | Yes | Path to the file to edit (relative or absolute). |
| `edits` | `ARRAY` | Yes | A list of targeted replacements. |

#### `edits` Item Structure
| Field | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `oldText` | `STRING` | Yes | Exact text for one targeted replacement. Must be unique in the original file and must not overlap with other edits in the same call. |
| `newText` | `STRING` | Yes | Replacement text for this targeted edit. |

### Behavioral Constraints
- Edits are matched against the original file, not incrementally.
- Overlapping or nested edits are forbidden.
- Nearby changes should be merged into one edit.
- `oldText` should be as small as possible while remaining unique.
- Do not include large unchanged regions to connect distant changes.

---

## 4. `write`
Writes content to a file.

### Arguments
| Argument | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `path` | `STRING` | Yes | Path to the file to write (relative or absolute). |
| `content` | `STRING` | Yes | Content to write to the file. |

### Behavioral Constraints
- Creates the file if it doesn't exist.
- Overwrites the file if it already exists.
- Automatically creates parent directories.
- Should only be used for new files or complete rewrites.
