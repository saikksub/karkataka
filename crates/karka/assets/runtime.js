import { createComponent as e, insert as t, memo as n, template as r } from "solid-js/web";
import { Show as i, createContext as a, createSignal as o, useContext as s } from "solid-js";
// ../runtime/src/math.ts
function c(e, t, n, { easing: r = (e) => e, clamp: i = !0 } = {}) {
	if (t.length < 2 || t.length !== n.length) throw Error("interpolate: inputRange and outputRange need the same length, at least 2");
	for (let e = 1; e < t.length; e++) if (!(t[e] > t[e - 1])) throw Error("interpolate: inputRange must increase");
	let a = 0;
	for (; a < t.length - 2 && e >= t[a + 1];) a++;
	let o = (e - t[a]) / (t[a + 1] - t[a]);
	return i && (o = Math.min(1, Math.max(0, o))), n[a] + (n[a + 1] - n[a]) * r(o);
}
function l({ frame: e, fps: t = 30, from: n = 0, to: r = 1, stiffness: i = 100, damping: a = 10, mass: o = 1 }) {
	if (e <= 0) return n;
	let s = e / t, c = Math.sqrt(i / o), l = a / (2 * Math.sqrt(i * o)), u;
	if (l < 1) {
		let e = c * Math.sqrt(1 - l * l);
		u = -Math.exp(-l * c * s) * (Math.cos(e * s) + l * c / e * Math.sin(e * s));
	} else if (l === 1) u = -Math.exp(-c * s) * (1 + c * s);
	else {
		let e = c * Math.sqrt(l * l - 1), t = -l * c + e, n = -l * c - e, r = t / (n - t);
		u = (-1 - r) * Math.exp(t * s) + r * Math.exp(n * s);
	}
	return n + (r - n) * (1 + u);
}
// ../runtime/src/index.tsx
var u = /*#__PURE__*/ r("<div style=position:absolute;top:250px;right:200px;bottom:440px;left:96px>");
function d() {
	try {
		return JSON.parse(new URLSearchParams(location.search).get("karka") ?? "{}");
	} catch {
		return {};
	}
}
var f = window.__karka ?? {
	props: {},
	frames: 300,
	fps: 30,
	width: 1920,
	height: 1080,
	...d()
};
function p(e, t) {
	return window.__karkaScene = {
		defaults: e,
		schema: t
	}, {
		...e,
		...f.props
	};
}
var [m, h] = o(0), g = [], _ = !1;
function v(e) {
	g.push(e.catch((e) => console.error("karka: a frame waited for something that failed:", e)));
}
function y() {
	return _;
}
window.__setFrame = (e) => {
	if (h(e), g.length === 0) return;
	let t = g;
	return g = [], Promise.all(t).then(() => void 0);
};
var b = a({
	start: 0,
	end: Infinity
});
function x() {
	let { start: e } = s(b);
	return () => m() - e;
}
function S() {
	let { start: e, end: t } = s(b);
	return (Number.isFinite(t) ? t : f.frames) - e;
}
function C(t) {
	let r = s(b), a = r.start + t.from, o = Math.min(r.end, a + (t.duration ?? Infinity));
	return e(b.Provider, {
		value: {
			start: a,
			end: o
		},
		get children() {
			return e(i, {
				get when() {
					return n(() => m() >= a)() && m() < o;
				},
				get children() {
					return t.children;
				}
			});
		}
	});
}
var w = a(""), T = /* @__PURE__ */ new Map();
function E(e) {
	let t = s(b), n = s(w), r = n && !e.src.startsWith("/") && !e.src.includes("://") ? n + e.src : e.src, i = t.start + (e.start ?? 0), [a = 0, o] = e.trim ?? [], c = Math.min(t.end, o === void 0 ? Infinity : i + o - a), l = {
		src: r,
		from: i,
		to: Number.isFinite(c) ? c : null,
		skip: a,
		volume: e.volume ?? 1
	};
	return T.set(JSON.stringify(l), l), null;
}
window.__karkaReport = () => {
	_ = !0;
	try {
		for (let e = 0; e < f.frames; e++) h(e);
	} finally {
		_ = !1, g = [];
	}
	return h(0), JSON.stringify({ audio: [...T.values()] });
};
function D(e) {
	return (() => {
		var n = u();
		return t(n, () => e.children), n;
	})();
}
export { w as AssetBase, E as Audio, D as SafeArea, C as Sequence, f as config, p as getProps, c as interpolate, y as isReporting, l as spring, S as useDuration, x as useFrame, v as waitFor };
