import React from "react";

// ── Shared markdown renderer ──────────────────────────────────────────────────
// Renders a markdown string as styled React elements.
// Supports: fenced code blocks, h1/h2/h3, tables, unordered/ordered lists,
// horizontal rules, inline code, bold, italic, and paragraphs.

export function MarkdownPreview({ content }: { content: string }) {
  // Inline formatter
  function inlineFormat(text: string): React.ReactElement {
    const parts: (string | React.ReactElement)[] = [];
    const regex = /(`[^`]+`|\*\*[^*]+\*\*|__[^_]+__|_[^_]+_|\*[^*]+\*)/g;
    let last = 0;
    let key = 0;
    let m: RegExpExecArray | null;
    while ((m = regex.exec(text)) !== null) {
      if (m.index > last) parts.push(text.slice(last, m.index));
      const t = m[0]!;
      if (t.startsWith("`"))
        parts.push(<code key={key++} className="px-1 py-0.5 rounded bg-[#1A1A1E] text-[#5E6AD2] font-mono text-[11px]">{t.slice(1, -1)}</code>);
      else if (t.startsWith("**") || t.startsWith("__"))
        parts.push(<strong key={key++} className="font-semibold text-[#F8F8FA]">{t.slice(2, -2)}</strong>);
      else
        parts.push(<em key={key++} className="italic text-[#E5E6EA]">{t.slice(1, -1)}</em>);
      last = m.index + t.length;
    }
    if (last < text.length) parts.push(text.slice(last));
    return <span>{parts}</span>;
  }

  const lines = content.split("\n");
  const elements: React.ReactElement[] = [];
  let i = 0;
  let k = 0;

  while (i < lines.length) {
    const line = lines[i]!;

    if (line.startsWith("```")) {
      const lang = line.slice(3).trim();
      const codeLines: string[] = [];
      i++;
      while (i < lines.length && !lines[i]!.startsWith("```")) { codeLines.push(lines[i]!); i++; }
      elements.push(
        <pre key={k++} className="mt-2 mb-4 bg-[#1A1A1E] border border-[#33353A] rounded-md px-4 py-3 font-mono text-[11px] text-[#E5E6EA] leading-relaxed overflow-x-auto whitespace-pre">
          {lang && <span className="block text-[10px] text-[#C8CAD0] mb-2 uppercase tracking-wider">{lang}</span>}
          {codeLines.join("\n")}
        </pre>
      );
      i++; continue;
    }
    if (line.startsWith("# ")) { elements.push(<h1 key={k++} className="text-[18px] font-semibold text-[#F8F8FA] mt-5 mb-2">{inlineFormat(line.slice(2))}</h1>); i++; continue; }
    if (line.startsWith("## ")) { elements.push(<h2 key={k++} className="text-[14px] font-semibold text-[#F8F8FA] mt-5 mb-2 pb-1.5 border-b border-[#33353A]">{inlineFormat(line.slice(3))}</h2>); i++; continue; }
    if (line.startsWith("### ")) { elements.push(<h3 key={k++} className="text-[13px] font-semibold text-[#F8F8FA] mt-4 mb-1.5">{inlineFormat(line.slice(4))}</h3>); i++; continue; }

    if (line.includes("|") && i + 1 < lines.length && /^\|?[\s|:-]+\|/.test(lines[i + 1]!)) {
      const parseRow = (row: string) => row.replace(/^\|/, "").replace(/\|$/, "").split("|").map(c => c.trim());
      const headers = parseRow(line);
      i += 2;
      const rows: string[][] = [];
      while (i < lines.length && lines[i]!.includes("|")) { rows.push(parseRow(lines[i]!)); i++; }
      elements.push(
        <div key={k++} className="my-4 overflow-x-auto">
          <table className="w-full text-[12px] border-collapse">
            <thead><tr>{headers.map((h, hi) => <th key={hi} className="text-left px-3 py-2 text-[#F8F8FA] font-semibold border-b border-[#33353A] bg-[#1A1A1E] whitespace-nowrap">{inlineFormat(h)}</th>)}</tr></thead>
            <tbody>{rows.map((cells, ri) => <tr key={ri} className={ri % 2 === 0 ? "bg-[#222327]" : "bg-[#1E1F23]"}>{cells.map((cell, ci) => <td key={ci} className="px-3 py-2 text-[#E5E6EA] border-b border-[#33353A]/40 align-top">{inlineFormat(cell)}</td>)}</tr>)}</tbody>
          </table>
        </div>
      );
      continue;
    }

    if (/^[-*] /.test(line)) {
      const items: React.ReactElement[] = [];
      while (i < lines.length && /^[-*] /.test(lines[i]!)) {
        items.push(<li key={k++} className="flex gap-2 text-[13px] text-[#E5E6EA] leading-relaxed"><span className="mt-1.5 w-1 h-1 rounded-full bg-[#C8CAD0] flex-shrink-0" /><span>{inlineFormat(lines[i]!.replace(/^[-*] /, ""))}</span></li>);
        i++;
      }
      elements.push(<ul key={k++} className="my-2 space-y-1.5 pl-1">{items}</ul>);
      continue;
    }
    if (/^\d+\. /.test(line)) {
      const items: React.ReactElement[] = [];
      let n = 1;
      while (i < lines.length && /^\d+\. /.test(lines[i]!)) {
        items.push(<li key={k++} className="flex gap-2.5 text-[13px] text-[#E5E6EA] leading-relaxed"><span className="flex-shrink-0 text-[11px] text-[#C8CAD0] font-mono w-4 text-right mt-0.5">{n}.</span><span>{inlineFormat(lines[i]!.replace(/^\d+\. /, ""))}</span></li>);
        i++; n++;
      }
      elements.push(<ol key={k++} className="my-2 space-y-1.5">{items}</ol>);
      continue;
    }
    if (/^---+$/.test(line.trim())) { elements.push(<hr key={k++} className="my-4 border-[#33353A]" />); i++; continue; }
    if (line.trim() === "") { i++; continue; }
    elements.push(<p key={k++} className="text-[13px] text-[#E5E6EA] leading-relaxed my-2">{inlineFormat(line)}</p>);
    i++;
  }

  return <div className="px-6 py-5">{elements}</div>;
}
