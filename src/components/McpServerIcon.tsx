import { useState } from "react";
import { Server } from "lucide-react";

const BRANDFETCH_CLIENT_ID = import.meta.env.VITE_BRANDFETCH_CLIENT_ID as string | undefined;

function brandfetchUrl(domain: string, px: number): string {
  const s = Math.min(px * 2, 64);
  return `https://cdn.brandfetch.io/${encodeURIComponent(domain)}/w/${s}/h/${s}/theme/dark/fallback/lettermark/type/icon?c=${BRANDFETCH_CLIENT_ID ?? ""}`;
}

interface McpServerIconProps {
  /** Brand domain from the Discover catalogue, e.g. "aikido.dev". */
  icon?: string | null;
  /** Alt text for the logo. */
  title: string;
  size: number;
}

/**
 * An MCP server's brand logo from Brandfetch, used by Discover and the MCP
 * Servers page. Falls back to the amber server icon when there is no domain,
 * no Brandfetch client id, or the image fails to load.
 */
export function McpServerIcon({ icon, title, size }: McpServerIconProps) {
  const [failedIcon, setFailedIcon] = useState<string | null>(null);

  if (icon && BRANDFETCH_CLIENT_ID && failedIcon !== icon) {
    return (
      <img
        src={brandfetchUrl(icon, size)}
        alt={title}
        width={size}
        height={size}
        onError={() => setFailedIcon(icon)}
        className="flex-shrink-0 rounded-md object-contain"
        style={{ width: size, height: size }}
        draggable={false}
      />
    );
  }

  return (
    <div
      className="flex-shrink-0 rounded-md bg-icon-mcp/10 border border-icon-mcp/20 flex items-center justify-center"
      style={{ width: size, height: size }}
    >
      <Server size={Math.round(size * 0.5)} style={{ color: "var(--icon-mcp)" }} />
    </div>
  );
}
