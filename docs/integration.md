# Integration Guide — Wiring into opencode Agents

How to register the athenaeum-mcp server with opencode and use it in your brainstorm, spar, and planner agents.

---

## What the Server Exposes

The MCP server provides two tools over stdio:

### Tool 1: `search`

**Description:** Search the personal library for cited passages.

**Input schema:**
```json
{
  "query": "string (natural-language query)",
  "k": "integer (max number of passages to return)"
}
```

**Output:** JSON array of `SearchHit` objects:
```json
[
  {
    "source": "algorithms.pdf",
    "location": "page 42",
    "text": "Machine learning is the study of algorithms that improve through experience...",
    "score": 0.87
  },
  {
    "source": "book.epub",
    "location": "Chapter 3 > Advanced Topics",
    "text": "In advanced machine learning, we consider...",
    "score": 0.82
  }
]
```

**Fields:**
- `source` — Document title (filename stem or EPUB metadata).
- `location` — Human-readable location (page number for PDFs, chapter/section for EPUBs).
- `text` — The raw passage text.
- `score` — Similarity score, range [0, 1] (1 = perfect match).

### Tool 2: `ingest_file`

**Description:** Ingest a PDF or EPUB file into the personal library.

**Input schema:**
```json
{
  "path": "string (absolute or relative path to PDF/EPUB)"
}
```

**Output:** JSON summary:
```json
{
  "documents": 1,
  "chunks": 42
}
```

**Fields:**
- `documents` — Number of documents ingested (always 1 for single-file ingestion).
- `chunks` — Number of text chunks created.

### Server Instructions

The server advertises itself with:
```
Personal library semantic search over CS, FP, and computer-graphics books and papers.
```

---

## Registering the Server in `opencode.json`

Add an MCP server entry to your `opencode.json`:

```json
{
  "mcp": {
    "athenaeum": {
      "type": "local",
      "command": "nix",
      "args": [
        "develop",
        "/path/to/athenaeum-mcp",
        "--command",
        "cargo",
        "run",
        "-p",
        "athenaeum-mcp-server"
      ],
      "cwd": "/path/to/athenaeum-mcp",
      "env": {}
    }
  }
}
```

**Key points:**
- `type: "local"` — runs a local command.
- `command` — `nix` (to enter the dev shell).
- `args` — `develop`, repo path, `--command`, then the cargo command.
- `cwd` — **must be the repo root** so the relative `db_path` (`./data/athenaeum`) resolves correctly.
- `env` — empty (no environment-variable overrides).

### Alternative: Prebuilt Binary

If you prefer to skip the Nix shell on every invocation, build a release binary:

```bash
nix develop --command cargo build --release -p athenaeum-mcp-server
```

Then register it:

```json
{
  "mcp": {
    "athenaeum": {
      "type": "local",
      "command": "/path/to/athenaeum-mcp/target/release/athenaeum-mcp-server",
      "cwd": "/path/to/athenaeum-mcp",
      "env": {}
    }
  }
}
```

---

## Scoping the Tool to Specific Agents

By default, MCP tools are available to all agents. To restrict `search` and `ingest_file` to only brainstorm, spar, and planner:

### Global Disable

In the global `mcp` block, disable the tools by default:

```json
{
  "mcp": {
    "athenaeum": {
      "type": "local",
      "command": "...",
      "tools": {
        "mcp*": false
      }
    }
  }
}
```

### Per-Agent Enable

In each agent's config, enable the tools:

```json
{
  "agents": {
    "brainstorm": {
      "tools": {
        "mcp/athenaeum/search": true,
        "mcp/athenaeum/ingest_file": true
      }
    },
    "spar": {
      "tools": {
        "mcp/athenaeum/search": true,
        "mcp/athenaeum/ingest_file": true
      }
    },
    "planner": {
      "tools": {
        "mcp/athenaeum/search": true,
        "mcp/athenaeum/ingest_file": true
      }
    }
  }
}
```

**Coding agents** (e.g., `build`, `reviewer`) do not get these tools, per the design decision: "coding agents do not access the corpus."

---

## Prompt Patterns Per Agent

One tool, three intents. Each agent should shape its own usage via prompt instructions.

### Brainstorm Agent: Extend & Support

**Intent:** Use the corpus to extend and develop ideas; retrieve supporting material.

**Prompt pattern:**
```
When developing ideas, use the search tool to find supporting material from the personal library.
Query for concepts, theories, and prior work that relate to the current discussion.
Always cite the source and location of retrieved passages.

Example:
  search("functional programming patterns", k=5)
  → Use results to expand on FP concepts in your response.
```

**Example usage:**
```
User: "I'm thinking about how to structure a parser combinator library."

Agent: "Let me search the library for relevant patterns..."
  search("parser combinators functional programming", k=5)
  
Agent: "Great! I found several relevant sources:
  - From 'Parsing Techniques' (page 87): 'Parser combinators are higher-order functions...'
  - From 'Functional Programming in Scala' (Chapter 9): 'Combinator libraries allow...'
  
  Based on these, here are some patterns to consider..."
```

### Spar Agent: Generate Friction

**Intent:** Use the corpus to generate friction and counter-arguments; challenge the user's thinking.

**Prompt pattern:**
```
When sparring, search the library for alternative viewpoints, critiques, and counter-arguments.
Retrieve material that challenges or complicates the current discussion.
Always cite the source and location of retrieved passages.

Example:
  search("criticism of X", k=5)
  search("alternative to X", k=5)
  → Use results to present friction and push back on assumptions.
```

**Example usage:**
```
User: "I think we should use a NoSQL database for this project."

Agent: "Let me search for some friction on that idea..."
  search("NoSQL limitations trade-offs", k=5)
  search("relational database advantages", k=5)
  
Agent: "Interesting. I found some counter-arguments:
  - From 'Designing Data-Intensive Applications' (Chapter 2): 'NoSQL databases sacrifice ACID guarantees...'
  - From 'The Art of SQL' (page 156): 'Relational databases provide...'
  
  Have you considered these trade-offs? What about consistency requirements?"
```

### Planner Agent: Factual Lookup

**Intent:** Use the corpus for factual lookup when questions arise during planning.

**Prompt pattern:**
```
When planning, use search to look up factual information, definitions, and technical details.
Retrieve precise, cited information to ground planning decisions.
Always cite the source and location of retrieved passages.

Example:
  search("definition of X", k=3)
  search("how to implement X", k=5)
  → Use results to provide accurate, grounded information.
```

**Example usage:**
```
User: "How should we structure the build system for a Rust project?"

Agent: "Let me look up best practices..."
  search("Rust Cargo workspace structure best practices", k=5)
  
Agent: "According to the Rust community:
  - From 'The Rust Book' (Chapter 14): 'Workspaces are useful for organizing multiple related packages...'
  - From 'Cargo documentation' (Workspaces): 'A workspace is a set of packages that share...'
  
  Here's a recommended structure for your project..."
```

---

## Usage Etiquette

### Retrieval is Explicitly Triggered

The corpus is **not always-on** or proactive. Agents must explicitly call `search()` when they want to retrieve material. This keeps the system lightweight and focused.

### Phrasing Queries

Write queries as natural-language questions or topics:

**Good:**
- `"machine learning algorithms"`
- `"how to design a type system"`
- `"functional programming patterns"`

**Avoid:**
- Single words: `"machine"` (too broad)
- Overly specific: `"page 42 of algorithms.pdf"` (the search tool doesn't know page numbers)
- Jargon without context: `"monad transformers"` (better: `"monad transformers in Haskell"`)

### Choosing `k` (Number of Results)

- **`k=3`** — Quick lookup, high confidence needed.
- **`k=5`** — Balanced; typical choice.
- **`k=10`** — Exploratory; find diverse perspectives.

### Citing Results

Always include the `source` and `location` in your response:

**Good:**
```
According to "Algorithms" (page 42): "Machine learning is..."
```

**Avoid:**
```
Machine learning is... (no citation)
```

---

## Verification: Test the Integration

### 1. Verify the Server Starts

```bash
nix develop --command cargo run -p athenaeum-mcp-server
```

You should see:
```
[listening on stdio]
```

### 2. Ingest a Test Document

```bash
nix develop --command cargo run -p athenaeum-ingest -- /path/to/test/document.pdf
```

### 3. Invoke Search Through an Agent

In opencode, invoke the brainstorm agent and ask it to search:

```
User: "Search the library for information about functional programming."

Agent: "Let me search the library..."
  [calls search("functional programming", k=5)]
  
Agent: "I found several relevant sources:
  - From 'Functional Programming in Scala' (Chapter 1): '...'
  - From 'The Art of Computer Programming' (page 123): '...'
  
  Here's what I learned..."
```

### 4. Confirm Citations

Verify that:
- Results include `source` and `location`.
- The `text` matches the actual document.
- Scores are in range [0, 1].

---

## Troubleshooting

### Server Fails to Start

**Error:** `Failed to initialize search engine`

**Likely cause:** Ollama not running or model not pulled.

**Fix:**
```bash
ollama serve
ollama pull nomic-embed-text
```

### Search Returns No Results

**Likely cause:** No documents ingested yet.

**Fix:**
```bash
nix develop --command cargo run -p athenaeum-ingest -- /path/to/documents
```

### Search Returns Results But Agent Doesn't Use Them

**Likely cause:** Agent prompt doesn't instruct the agent to call `search()`.

**Fix:** Add explicit instructions to the agent's prompt (see "Prompt Patterns Per Agent" above).

### Tool Not Available in Agent

**Likely cause:** Tool not scoped to the agent in `opencode.json`.

**Fix:** Check the `tools` configuration for the agent (see "Scoping the Tool to Specific Agents" above).

---

## Context Cost & Performance

### Token Usage

Each `search()` call:
- Sends the query to Ollama for embedding (~10–50 tokens).
- Retrieves up to `k` passages from LanceDB (~100–500 tokens per passage).
- Total per call: ~1–5K tokens (depends on `k` and passage length).

**Recommendation:** Use `k=5` as a default; increase to `k=10` only for exploratory queries.

### Latency

- **Embedding the query:** ~100–500ms (depends on Ollama hardware).
- **LanceDB search:** ~10–50ms (depends on database size).
- **Total:** ~200–600ms per call.

This is acceptable for explicit, on-demand retrieval but not for always-on background queries.

---

## Next Steps

- **[setup.md](setup.md)** — Verify your environment is ready.
- **[ingestion.md](ingestion.md)** — Load your corpus.
- **[architecture.md](architecture.md)** — Understand the system design.
