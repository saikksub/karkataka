import { createContext, For, useContext } from "solid-js";
import { Audio, config, interpolate, SafeArea, Sequence, spring, useDuration, useFrame, type PropsSchema } from "@karkataka/runtime";
import defaults from "../props.json";

// A 9:16 quiz short: question, code, four options, a countdown with ticks, then the answer with a
// chime. Use it as a component clip in a 1080x1920 project; its length is the clip's duration.

export type QuizProps = typeof defaults;
export { defaults };

/** How the Studio's props panel edits the props. */
export const schema: PropsSchema = {
  number: { type: "integer", title: "Quiz number", minimum: 1 },
  question: { type: "string", title: "Question" },
  code: { type: "string", title: "Code", multiline: true },
  options: { type: "array", title: "Options", items: { type: "string" }, description: "Four answers, A to D." },
  answer: { type: "integer", title: "Correct option", minimum: 0, maximum: 3, description: "0 = A, 1 = B, 2 = C, 3 = D." },
  explanation: { type: "string", title: "Explanation", multiline: true },
};

const letters = "ABCD";

/** Frames where each part starts, for a quiz `frames` long. The reveal gets the last 3 seconds; the timer counts down until then. */
export function timeline(props: QuizProps, frames: number, fps: number) {
  const codeAt = 12;
  const optionsAt = codeAt + props.code.split("\n").length * 4 + 8;
  const timerAt = optionsAt + props.options.length * 6 + 8;
  return { codeAt, optionsAt, timerAt, revealAt: Math.max(timerAt + fps, frames - 3 * fps) };
}

type Quiz = ReturnType<typeof timeline> & { props: QuizProps; fps: number; lines: string[] };
const QuizContext = createContext<Quiz>();
const useQuiz = () => useContext(QuizContext)!;

const fadeUp = (s: number, px = 50) => ({ opacity: `${Math.min(1, s)}`, transform: `translateY(${(1 - s) * px}px)` });

export default function Quiz(input: Partial<QuizProps>) {
  const props = { ...defaults, ...input };
  const fps = config.fps;
  const quiz: Quiz = { ...timeline(props, useDuration(), fps), props, fps, lines: props.code.split("\n") };
  const { codeAt, optionsAt, timerAt, revealAt } = quiz;
  return (
    <QuizContext.Provider value={quiz}>
      {/* Full-bleed background; everything readable sits in the safe area. */}
      <div style={{ position: "absolute", inset: "0", background: "radial-gradient(circle at 30% 20%, #1e2a4a, #0b0f1c 70%)", color: "#e8ecf6", "font-family": "-apple-system, Helvetica, Arial, sans-serif" }}>
      <SafeArea>
        <div style={{ display: "flex", "flex-direction": "column", gap: "36px", height: "100%" }}>
          <Header />
          <Sequence from={codeAt}>
            <Code />
          </Sequence>
          <Sequence from={optionsAt}>
            <Options />
          </Sequence>
          <div style={{ position: "relative", flex: "1" }}>
            <Sequence from={timerAt} duration={revealAt - timerAt}>
              <Timer total={revealAt - timerAt} />
            </Sequence>
            <Sequence from={revealAt}>
              <Reveal />
            </Sequence>
          </div>
        </div>
      </SafeArea>
      </div>
    </QuizContext.Provider>
  );
}

function Header() {
  const { props, fps } = useQuiz();
  const f = useFrame();
  const s = () => spring({ frame: f(), fps, damping: 14 });
  return (
    <div style={fadeUp(s())}>
      <div style={{ "font-size": "30px", "font-weight": "700", "letter-spacing": "6px", color: "#7aa2ff" }}>C QUIZ #{props.number}</div>
      <div style={{ "font-size": "58px", "font-weight": "800", "margin-top": "12px" }}>{props.question}</div>
    </div>
  );
}

function Code() {
  const { fps, lines } = useQuiz();
  const f = useFrame();
  // Fit the longest line into the panel (monospace glyphs are ~0.6em wide).
  const size = Math.min(40, (784 - 64) / (Math.max(...lines.map((l) => l.length)) * 0.6));
  return (
    <div
      style={{
        ...fadeUp(spring({ frame: f(), fps, damping: 16 }), 80),
        background: "#11172a",
        border: "2px solid #2a3557",
        "border-radius": "24px",
        padding: "28px 32px",
        "font-family": "Menlo, Monaco, monospace",
        "font-size": `${size}px`,
        "line-height": "1.5",
      }}
    >
      <For each={lines}>
        {(line, i) => <div style={{ "white-space": "pre", opacity: `${interpolate(f(), [i() * 4, i() * 4 + 8], [0, 1])}` }}>{line || " "}</div>}
      </For>
    </div>
  );
}

function Options() {
  const { props, fps, revealAt, optionsAt } = useQuiz();
  const f = useFrame();
  // 0 until the reveal, then springs to 1.
  const revealed = () => spring({ frame: f() - (revealAt - optionsAt), fps, damping: 12 });
  return (
    <div style={{ display: "grid", "grid-template-columns": "1fr 1fr", gap: "20px" }}>
      <For each={props.options}>
        {(option, i) => {
          const s = () => spring({ frame: f() - i() * 6, fps, damping: 14 });
          const right = i() === props.answer;
          return (
            <div
              style={{
                ...fadeUp(s(), 40),
                position: "relative",
                overflow: "hidden",
                "border-radius": "20px",
                border: `3px solid ${right ? "#3ddc84" : "#2a3557"}`,
                "border-color": right && revealed() > 0 ? "#3ddc84" : "#2a3557",
                background: "#151c33",
                padding: "26px 28px",
                "font-size": "40px",
                "font-weight": "700",
                scale: right ? `${1 + 0.06 * revealed()}` : "1",
                filter: right ? "none" : `opacity(${1 - 0.6 * Math.min(1, revealed())})`,
              }}
            >
              <div style={{ position: "absolute", inset: "0", background: "#1f7a4a", opacity: right ? `${Math.min(1, revealed())}` : "0" }} />
              <span style={{ position: "relative" }}>
                <span style={{ color: "#7aa2ff", "margin-right": "18px" }}>{letters[i()]}</span>
                <span style={{ "font-family": "Menlo, Monaco, monospace" }}>{option}</span>
              </span>
            </div>
          );
        }}
      </For>
    </div>
  );
}

function Timer(p: { total: number }) {
  const { fps } = useQuiz();
  const f = useFrame();
  const r = 110;
  const length = 2 * Math.PI * r;
  const left = () => Math.ceil((p.total - f()) / fps);
  const urgent = () => left() <= 3;
  // Each second the number pops and settles.
  const pop = () => 1 + 0.18 * (1 - spring({ frame: (p.total - f()) % fps === 0 ? 0 : fps - ((p.total - f()) % fps), fps, damping: 18 }));
  // One tick at the start of each second.
  const ticks = Array.from({ length: Math.ceil(p.total / fps) }, (_, s) => s * fps);
  return (
    <div style={{ position: "absolute", inset: "0", display: "grid", "place-items": "center", opacity: `${interpolate(f(), [0, 8], [0, 1])}` }}>
      <For each={ticks}>{(at) => <Audio src="media/tick.wav" start={at} volume={0.6} />}</For>
      <svg width="260" height="260" viewBox="0 0 260 260" style={{ position: "absolute" }}>
        <circle cx="130" cy="130" r={r} fill="none" stroke="#1f2742" stroke-width="16" />
        <circle
          cx="130"
          cy="130"
          r={r}
          fill="none"
          stroke={urgent() ? "#ff5a5a" : "#ffc24a"}
          stroke-width="16"
          stroke-linecap="round"
          stroke-dasharray={`${length}`}
          stroke-dashoffset={`${length * interpolate(f(), [0, p.total], [0, 1])}`}
          transform="rotate(-90 130 130)"
        />
      </svg>
      <div style={{ "font-size": "110px", "font-weight": "800", color: urgent() ? "#ff5a5a" : "#e8ecf6", scale: `${pop()}` }}>{left()}</div>
    </div>
  );
}

function Reveal() {
  const { props, fps } = useQuiz();
  const f = useFrame();
  return (
    <div style={{ ...fadeUp(spring({ frame: f() - 6, fps, damping: 14 })), position: "absolute", inset: "0", display: "flex", "flex-direction": "column", "justify-content": "center", "text-align": "center" }}>
      <Audio src="media/ding.wav" />
      <div style={{ "font-size": "56px", "font-weight": "800", color: "#3ddc84" }}>
        Answer: {letters[props.answer]} · {props.options[props.answer]}
      </div>
      <div style={{ "font-size": "36px", "margin-top": "20px", color: "#b9c3dd" }}>{props.explanation}</div>
    </div>
  );
}
