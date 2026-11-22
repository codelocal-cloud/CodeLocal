export const socialImageAlt = "CodeLocal — AI coding with local execution and durable project intelligence";
export const socialImageSize = { width: 1200, height: 630 } as const;

export function SocialCard({ brandIconSrc }: { brandIconSrc: string }) {
  return (
    <div
      style={{
        width: "100%",
        height: "100%",
        display: "flex",
        position: "relative",
        overflow: "hidden",
        background: "linear-gradient(135deg, #030609 0%, #07111b 52%, #081723 100%)",
        color: "#f8fbff",
        fontFamily: "Arial, Helvetica, sans-serif",
      }}
    >
      <div
        style={{
          position: "absolute",
          width: 520,
          height: 520,
          right: -120,
          top: -170,
          borderRadius: 520,
          background: "radial-gradient(circle, rgba(45, 212, 255, 0.24) 0%, rgba(45, 212, 255, 0.02) 58%, rgba(45, 212, 255, 0) 74%)",
          display: "flex",
        }}
      />
      <div
        style={{
          position: "absolute",
          width: 420,
          height: 420,
          left: 360,
          bottom: -310,
          borderRadius: 420,
          background: "radial-gradient(circle, rgba(35, 255, 169, 0.16) 0%, rgba(35, 255, 169, 0) 72%)",
          display: "flex",
        }}
      />
      <div
        style={{
          position: "absolute",
          inset: 28,
          border: "1px solid rgba(148, 190, 218, 0.16)",
          borderRadius: 28,
          display: "flex",
        }}
      />

      <div
        style={{
          width: "100%",
          height: "100%",
          padding: "66px 72px 58px",
          display: "flex",
          flexDirection: "column",
          justifyContent: "space-between",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 16 }}>
          <img
            src={brandIconSrc}
            alt=""
            width={58}
            height={58}
            style={{ width: 58, height: 58, objectFit: "contain", display: "flex" }}
          />
          <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
            <div style={{ fontSize: 29, fontWeight: 800, letterSpacing: -1 }}>CodeLocal</div>
            <div style={{ fontSize: 14, color: "#7f9bad", letterSpacing: 2.2, textTransform: "uppercase" }}>Local-first AI control plane</div>
          </div>
        </div>

        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 60 }}>
          <div style={{ width: 620, display: "flex", flexDirection: "column", gap: 24 }}>
            <div style={{ display: "flex", flexDirection: "column", fontSize: 66, lineHeight: 0.98, fontWeight: 800, letterSpacing: -3.8 }}>
              <span>Your AI.</span>
              <span>Your codebase.</span>
              <span style={{ color: "#66e3ff" }}>Your machine.</span>
            </div>
            <div style={{ fontSize: 24, lineHeight: 1.35, color: "#a9bcc9", maxWidth: 590 }}>
              Connect AI coding clients to an authorized local runtime and a durable Project Brain.
            </div>
          </div>

          <div
            style={{
              width: 350,
              borderRadius: 24,
              border: "1px solid rgba(116, 199, 232, 0.24)",
              background: "rgba(5, 15, 23, 0.78)",
              padding: "26px 24px",
              display: "flex",
              flexDirection: "column",
              gap: 15,
              boxShadow: "0 30px 80px rgba(0,0,0,0.32)",
            }}
          >
            {[
              ["AI CLIENT", "ChatGPT · Claude · Codex"],
              ["CODELOCAL", "Project Brain · Policy · MCP"],
              ["LOCAL RUNTIME", "Files · Terminal · Browser"],
            ].map(([label, detail], index) => (
              <div key={label} style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                <div
                  style={{
                    minHeight: 64,
                    borderRadius: 16,
                    border: index === 1 ? "1px solid rgba(81, 219, 255, 0.52)" : "1px solid rgba(130, 158, 176, 0.16)",
                    background: index === 1 ? "linear-gradient(90deg, rgba(22, 91, 120, 0.38), rgba(16, 55, 73, 0.2))" : "rgba(12, 24, 34, 0.72)",
                    padding: "12px 15px",
                    display: "flex",
                    flexDirection: "column",
                    justifyContent: "center",
                    gap: 4,
                  }}
                >
                  <span style={{ fontSize: 12, color: index === 1 ? "#69e4ff" : "#7793a5", letterSpacing: 1.8 }}>{label}</span>
                  <span style={{ fontSize: 16, fontWeight: 700, color: "#e8f2f7" }}>{detail}</span>
                </div>
                {index < 2 && <div style={{ height: 10, display: "flex", justifyContent: "center", color: "#45dca7", fontSize: 15 }}>↓</div>}
              </div>
            ))}
          </div>
        </div>

        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
          <div style={{ display: "flex", gap: 12 }}>
            {["Open source", "Workspace-scoped", "Secure by design"].map((label) => (
              <div
                key={label}
                style={{
                  border: "1px solid rgba(130, 171, 196, 0.2)",
                  borderRadius: 999,
                  padding: "8px 13px",
                  fontSize: 13,
                  color: "#9bb2c1",
                  display: "flex",
                }}
              >
                {label}
              </div>
            ))}
          </div>
          <div style={{ fontSize: 16, color: "#6f8b9d", display: "flex" }}>codelocal.cloud</div>
        </div>
      </div>
    </div>
  );
}
