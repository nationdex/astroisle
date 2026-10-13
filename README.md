# astroisle 🏝️

**Micro-Islands Bundler & Asset Delivery Pipeline for Alpine / Zero-JS**

Zero-allocation, SIMD-accelerated, automated client asset delivery pipeline built for ultra-strict budgets (< 104 KiB initial JS) and high-throughput web architectures (e.g. MindVote, Astro, Alpine, htmx).

---

## Capabilities

- ⚡ **Strict Budget Enforcement**: Audits initial JavaScript (< 104 KiB), lazy island chunks (< 70 KiB), and CSS (< 14 KiB) gzip budgets with zero tolerance.
- 🔒 **Subresource Integrity (SRI)**: Auto-computes and pins `integrity="sha384-..."` attributes for all `<script>` and `<link>` tags in HTML.
- 🚀 **Modulepreload Discovery**: Graph traces ES module dependency trees from entry scripts to eliminate network waterfalls.
- 📦 **Automated Hash-Verified Precompression**: Invokes `nstd` (or multi-threaded native fallback) to generate `.zst`, `.br`, and `.gz` assets. Decompresses every file in memory to verify SHA-256 byte-level integrity.
- 🛡️ **Source Map Leak Prevention**: Audits and halts on accidental `.map` file leakage or `sourceMappingURL=` comments.
- 👷 **Service Worker Manifest Generation**: Auto-injects `__VERSION__` and `__PRECACHE__` into `sw.js`.
- 🧩 **Micro-Islands & Web Components Auditor**: Discovers `data-island` attributes and custom elements across HTML, Astro, and Rust templates.

---

## Installation

### Cargo (Native Binary)
```bash
cargo install --path .
```

### Bun / Node
```bash
bun add -d astroisle
```

---

## CLI Usage

### Complete Pipeline
```bash
astroisle run --dist ./dist
```

### Audit Budgets
```bash
astroisle audit --dist ./dist --budget-initial 104 --budget-lazy 70 --budget-css 14
```

### Subresource Integrity & Modulepreload
```bash
astroisle integrity --dist ./dist
```

### Precompress with nstd
```bash
astroisle compress --dist ./dist --level 9
```

### Scan Micro-Islands
```bash
astroisle islands --path ./src
```

### Emit Alpine Micro-Islands Loader Runtime
```bash
astroisle runtime
```

---

## Astro Integration

```javascript
// astro.config.mjs
import { defineConfig } from "astro/config";
import astroisle from "astroisle";

export default defineConfig({
  integrations: [
    astroisle({
      budgetInitial: 104,
      budgetLazy: 70,
      budgetCss: 14,
    }),
  ],
});
```

---

## License

MIT OR Apache-2.0
