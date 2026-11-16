import { config, interpolate, spring, useDuration, useFrame } from "@karkataka/runtime";

// A scene: a SolidJS component drawn frame by frame. It fills its clip's box, and its props come
// from the clip's "props" in project.json. Draw only from the frame and the props: the same frame
// must always look the same (no Date.now(), Math.random() or timers).

/** How the Studio edits the props. */
export const schema = {
  title: { type: "string", title: "Title" },
};

export default function {Name}(props: { title?: string }) {
  const frame = useFrame(); // 0 at the clip's start
  const length = useDuration(); // the clip's length in frames
  const enter = () => spring({ frame: frame(), fps: config.fps, damping: 14 });
  const leave = () => interpolate(frame(), [length - 15, length], [1, 0]);
  return (
    <div
      style={{
        position: "absolute",
        inset: "0",
        display: "grid",
        "place-items": "center",
        background: "linear-gradient(135deg, #1e2a4a, #0b0f1c)",
        color: "#fff",
        "font-family": "-apple-system, Helvetica, Arial, sans-serif",
      }}
    >
      <h1 style={{ margin: "0", "font-size": "120px", opacity: `${leave()}`, transform: `translateY(${(1 - enter()) * 60}px)` }}>
        {props.title ?? "{Name}"}
      </h1>
    </div>
  );
}
