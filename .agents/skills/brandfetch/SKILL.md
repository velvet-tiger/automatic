---
name: "Brandfetch Brand Assets"
description: "Fetch brand assets (logos, colors, fonts, images, company info) from Brandfetch for any company by domain, ticker, ISIN, or crypto symbol. Use when retrieving logos, brand colors, brand guidelines, or company brand data; or when the user asks to fetch, download, or look up brand assets, logos, or visual identity for a company."
---

# Brandfetch Brand Assets

## What This Skill Does

Retrieves brand assets for any company using the [Brandfetch API](https://brandfetch.com/developers). A single API call returns:

- **Logos** — icon, logo, and symbol variants in SVG/PNG/WebP, dark and light themes
- **Colors** — accent, dark, light, and full brand palette with HEX codes
- **Fonts** — title and body fonts (Google, custom, or system)
- **Images** — banners and other brand images
- **Company info** — description, social links, industry, location, employee count, founding year

## Prerequisites

- A Brandfetch API key from [developers.brandfetch.com](https://developers.brandfetch.com/register)
- Set as `BRANDFETCH_API_KEY` in your environment, or provide it inline

## Quick Start

```bash
# Fetch brand data for a domain
curl --request GET \
  --url "https://api.brandfetch.io/v2/brands/domain/nextjs.org" \
  --header "Authorization: Bearer $BRANDFETCH_API_KEY"
```

Browse the brand page at `https://brandfetch.com/nextjs.org` to preview assets visually.

---

## API Reference

### Base URL

```
https://api.brandfetch.io/v2/brands
```

### Authentication

Pass your API key as a Bearer token:

```
Authorization: Bearer <BRANDFETCH_API_KEY>
```

> All requests to `brandfetch.com` itself are free and don't count toward quota — useful for testing.

### Identifier Types

| Type | Example | Endpoint |
|------|---------|----------|
| Domain | `nextjs.org` | `/v2/brands/domain/nextjs.org` |
| Stock/ETF ticker | `NKE` | `/v2/brands/ticker/NKE` |
| ISIN | `US6541061031` | `/v2/brands/isin/US6541061031` |
| Crypto symbol | `BTC` | `/v2/brands/crypto/BTC` |
| Auto-detect | `nextjs.org` | `/v2/brands/nextjs.org` |

Prefer explicit type routes (e.g. `/domain/`, `/ticker/`) to avoid identifier collisions.

---

## Step-by-Step Guide

### 1. Fetch brand data

```bash
export BRANDFETCH_API_KEY="your_api_key_here"

curl --request GET \
  --url "https://api.brandfetch.io/v2/brands/domain/nextjs.org" \
  --header "Authorization: Bearer $BRANDFETCH_API_KEY" \
  | jq .
```

### 2. Parse the response

Key fields in the JSON response:

```json
{
  "name": "Next.js",
  "domain": "nextjs.org",
  "description": "...",
  "logos": [
    {
      "type": "icon",          // "icon" | "logo" | "symbol" | "other"
      "theme": "dark",         // "dark" | "light" | null
      "formats": [
        {
          "src": "https://asset.brandfetch.io/...",
          "format": "svg",     // "svg" | "webp" | "png" | "jpeg"
          "width": 512,
          "height": 512,
          "background": "transparent"
        }
      ]
    }
  ],
  "colors": [
    {
      "hex": "#000000",
      "type": "accent",        // "accent" | "dark" | "light" | "brand"
      "brightness": 0
    }
  ],
  "fonts": [
    {
      "name": "Inter",
      "type": "body",          // "title" | "body"
      "origin": "google"       // "google" | "custom" | "system"
    }
  ],
  "company": {
    "foundedYear": 2016,
    "location": { "city": "San Francisco", "country": "United States" },
    "industries": [{ "name": "Software", "emoji": "💻" }]
  },
  "qualityScore": 0.92
}
```

### 3. Download a specific logo

Extract the `src` URL from `logos[].formats[].src` and download:

```bash
# Get the SVG icon URL from the response, then download it
curl -L "https://asset.brandfetch.io/..." -o nextjs-icon.svg
```

### 4. Use the Logo CDN (no API key for display)

For embedding logos directly in HTML, use the free Logo CDN:

```html
<!-- Default icon (auto-detect) -->
<img src="https://cdn.brandfetch.io/nextjs.org?c=YOUR_CLIENT_ID" alt="Next.js logo" />

<!-- Explicit domain, light theme, logo type, sized to 64x64 -->
<img
  src="https://cdn.brandfetch.io/domain/nextjs.org/theme/dark/logo?h=64&w=256&c=YOUR_CLIENT_ID"
  alt="Next.js logo"
/>

<!-- With fallback to lettermark if logo unavailable -->
<img
  src="https://cdn.brandfetch.io/nextjs.org/fallback/lettermark/icon?c=YOUR_CLIENT_ID"
  alt="Next.js logo"
/>
```

Logo CDN URL pattern:
```
https://cdn.brandfetch.io/{domain}[/theme/{dark|light}][/{icon|logo|symbol}][.{png|svg|jpeg}]?c={CLIENT_ID}
```

---

## Common Workflows

### Extract all logo URLs from a brand

```bash
curl -s "https://api.brandfetch.io/v2/brands/domain/nextjs.org" \
  -H "Authorization: Bearer $BRANDFETCH_API_KEY" \
  | jq '[.logos[] | {type, theme, formats: [.formats[] | {src, format}]}]'
```

### Get the brand's accent color

```bash
curl -s "https://api.brandfetch.io/v2/brands/domain/nextjs.org" \
  -H "Authorization: Bearer $BRANDFETCH_API_KEY" \
  | jq '.colors[] | select(.type == "accent") | .hex'
```

### Download the best SVG logo

```bash
SVG_URL=$(curl -s "https://api.brandfetch.io/v2/brands/domain/nextjs.org" \
  -H "Authorization: Bearer $BRANDFETCH_API_KEY" \
  | jq -r '[.logos[] | .formats[] | select(.format == "svg")] | first | .src')

curl -L "$SVG_URL" -o logo.svg
echo "Saved to logo.svg"
```

### Fetch multiple brands in a script

```bash
#!/usr/bin/env bash
domains=("nextjs.org" "vercel.com" "stripe.com")
for domain in "${domains[@]}"; do
  echo "Fetching $domain..."
  curl -s "https://api.brandfetch.io/v2/brands/domain/$domain" \
    -H "Authorization: Bearer $BRANDFETCH_API_KEY" \
    -o "${domain//./_}.json"
done
```

---

## Errors

| Status | Meaning | Action |
|--------|---------|--------|
| `400` | Bad request (malformed identifier) | Check domain/ticker format |
| `401` | Unauthorized | Verify `BRANDFETCH_API_KEY` is set and valid |
| `404` | Brand not found | Try auto-detect route; brand may not be indexed |
| `429` | Rate limit / quota exceeded | Check plan limits at [developers.brandfetch.com](https://developers.brandfetch.com) |

---

## Resources

- [Brandfetch Developer Portal](https://developers.brandfetch.com)
- [Brand API Docs](https://docs.brandfetch.com/brand-api/overview)
- [Logo CDN Docs](https://docs.brandfetch.com/logo-api/overview)
- [Logo CDN Parameters](https://docs.brandfetch.com/logo-api/parameters)
- [Brand Search API](https://docs.brandfetch.com/brand-search-api/overview) — autocomplete brand names to domains
- Preview any brand at `https://brandfetch.com/{domain}` (e.g. `https://brandfetch.com/nextjs.org`)
