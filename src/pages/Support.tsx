import { Bug, MessageCircle, ExternalLink } from "lucide-react";

const SUPPORT_CHANNELS = [
  {
    id: "github",
    label: "GitHub Issues",
    description: "Report bugs, request features, or browse existing issues on our GitHub repository.",
    icon: <Bug size={20} />,
    url: "https://github.com/velvet-tiger/automatic/issues",
  },
  {
    id: "discord",
    label: "Discord Community",
    description: "Join our Discord server to chat with the team and other users, ask questions, and share feedback.",
    icon: <MessageCircle size={20} />,
    url: "https://discord.gg/bAhmvZTmcC",
  },
];

export default function Support() {
  return (
    <div className="h-full overflow-y-auto custom-scrollbar">
      <div className="max-w-2xl mx-auto px-6 py-10">
        <h1 className="text-lg font-semibold text-text-base mb-1">Support</h1>
        <p className="text-[13px] text-text-muted mb-8">
          Need help? Choose one of the options below to get in touch.
        </p>

        <div className="space-y-3">
          {SUPPORT_CHANNELS.map((channel) => (
            <a
              key={channel.id}
              href={channel.url}
              target="_blank"
              rel="noopener noreferrer"
              className="block w-full text-left bg-bg-input border border-border-strong/40 rounded-lg p-5 hover:border-border-strong transition-colors group no-underline"
            >
              <div className="flex items-start gap-4">
                <div className="mt-0.5 text-text-muted group-hover:text-text-base transition-colors">
                  {channel.icon}
                </div>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2 mb-1">
                    <span className="text-[14px] font-medium text-text-base">
                      {channel.label}
                    </span>
                    <ExternalLink size={12} className="text-text-muted opacity-0 group-hover:opacity-100 transition-opacity" />
                  </div>
                  <p className="text-[13px] text-text-muted leading-relaxed">
                    {channel.description}
                  </p>
                </div>
              </div>
            </a>
          ))}
        </div>
      </div>
    </div>
  );
}
