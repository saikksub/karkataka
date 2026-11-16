import { Dynamic as e, createComponent as t, effect as n, insert as r, memo as i, mergeProps as a, render as o, setAttribute as s, setStyleProperty as c, style as l, template as u } from "solid-js/web";
import { For as d, Match as f, Show as p, Switch as m, createRenderEffect as h, createSignal as g } from "solid-js";
import { AssetBase as _, Audio as v, Sequence as y, interpolate as b, isReporting as x, useDuration as S, useFrame as C, waitFor as w } from "@karkataka/runtime";
// ../player/src/Player.tsx
var T = /*#__PURE__*/ u("<div style=position:relative;overflow:hidden>"), E = /*#__PURE__*/ u("<div style=position:absolute;overflow:hidden>"), D = /*#__PURE__*/ u("<div style=position:absolute;inset:0>"), O = /*#__PURE__*/ u("<img alt>"), k = /*#__PURE__*/ u("<video muted preload=auto playsinline>"), A = /*#__PURE__*/ u("<div style=position:absolute;inset:0;display:flex;flex-direction:column;white-space:pre-wrap>"), j = /*#__PURE__*/ u("<div style=\"position:absolute;inset:0;display:grid;place-items:center;background:#400;color:#fff;font:bold 32px sans-serif\">missing ");
function M(e) {
	return (() => {
		var a = T();
		return r(a, t(d, {
			get each() {
				return e.project.tracks;
			},
			children: (n) => t(d, {
				get each() {
					return n.clips;
				},
				children: (r) => t(y, {
					get from() {
						return r.start;
					},
					get duration() {
						return r.duration;
					},
					get children() {
						return [t(p, {
							get when() {
								return i(() => !(n.muted || !r.path))() && (r.type === "audio" || r.type === "video" && r.audio);
							},
							get children() {
								return t(v, {
									get src() {
										return r.path;
									},
									get trim() {
										return [r.trim ?? 0];
									},
									get volume() {
										return r.volume ?? 1;
									}
								});
							}
						}), t(p, {
							get when() {
								return i(() => !n.hidden)() && r.type !== "audio";
							},
							get children() {
								return t(N, {
									clip: r,
									get project() {
										return e.project;
									},
									get components() {
										return e.components;
									}
								});
							}
						})];
					}
				})
			})
		})), n((t) => {
			var n = `${e.project.width}px`, r = `${e.project.height}px`, i = e.project.background ?? "#000";
			return n !== t.e && c(a, "width", t.e = n), r !== t.t && c(a, "height", t.t = r), i !== t.a && c(a, "background", t.a = i), t;
		}, {
			e: void 0,
			t: void 0,
			a: void 0
		}), a;
	})();
}
function N(o) {
	let l = C(), u = S(), d = () => o.clip.box ?? {
		x: 0,
		y: 0,
		width: o.project.width,
		height: o.project.height
	}, h = () => {
		let { fadeIn: e = 0, fadeOut: t = 0, opacity: n = 1 } = o.clip, r = e > 0 ? b(l(), [0, e], [0, 1]) : 1, i = t > 0 ? b(l(), [u - t, u], [1, 0]) : 1;
		return n * r * i;
	};
	return (() => {
		var l = E();
		return r(l, t(m, { get children() {
			return [
				t(f, {
					get when() {
						return i(() => o.clip.type === "video")() && o.clip;
					},
					children: (e) => e().frames ? t(I, { get clip() {
						return e();
					} }) : t(L, {
						get clip() {
							return e();
						},
						get fps() {
							return o.project.fps;
						}
					})
				}),
				t(f, {
					get when() {
						return i(() => o.clip.type === "image")() && o.clip;
					},
					children: (e) => t(R, { get clip() {
						return e();
					} })
				}),
				t(f, {
					get when() {
						return i(() => o.clip.type === "text")() && o.clip;
					},
					children: (e) => t(z, {
						get text() {
							return e().text;
						},
						get style() {
							return e().style;
						}
					})
				}),
				t(f, {
					get when() {
						return i(() => o.clip.type === "color")() && o.clip;
					},
					children: (e) => (() => {
						var t = D();
						return n((n) => c(t, "background", e().color)), t;
					})()
				}),
				t(f, {
					get when() {
						return i(() => o.clip.type === "component")() && o.clip;
					},
					children: (n) => {
						let r = () => o.components[n().module ?? ""];
						return t(p, {
							get when() {
								return r()?.default;
							},
							get fallback() {
								return t(B, { get what() {
									return `component ${n().component}`;
								} });
							},
							get children() {
								return t(_.Provider, {
									get value() {
										return o.project.base ?? "";
									},
									get children() {
										return t(e, a({ get component() {
											return r().default;
										} }, () => n().props ?? {}));
									}
								});
							}
						});
					}
				})
			];
		} })), n((e) => {
			var t = o.clip.id, n = `${d().x}px`, r = `${d().y}px`, i = `${d().width}px`, a = `${d().height}px`, u = `${h()}`;
			return t !== e.e && s(l, "data-clip", e.e = t), n !== e.t && c(l, "left", e.t = n), r !== e.a && c(l, "top", e.a = r), i !== e.o && c(l, "width", e.o = i), a !== e.i && c(l, "height", e.i = a), u !== e.n && c(l, "opacity", e.n = u), e;
		}, {
			e: void 0,
			t: void 0,
			a: void 0,
			o: void 0,
			i: void 0,
			n: void 0
		}), l;
	})();
}
function P(e) {
	return new Promise((t, n) => {
		let r = () => e.decode().catch(() => {}).then(() => t());
		if (e.complete && e.naturalWidth > 0) return r();
		e.addEventListener("load", r, { once: !0 }), e.addEventListener("error", () => n(/* @__PURE__ */ Error(`cannot load image ${e.src}`)), { once: !0 });
	});
}
var F = (e) => ({
	width: "100%",
	height: "100%",
	display: "block",
	"object-fit": e ?? "cover"
});
function I(e) {
	let t = C(), r = (() => {
		var t = O();
		return n((n) => l(t, F(e.clip.fit), n)), t;
	})();
	return h(() => {
		let { url: n, first: i, count: a } = e.clip.frames, o = Math.min(Math.max(t() - i, 0), a - 1), s = `${n}${String(o + 1).padStart(6, "0")}.jpg`;
		x() || a === 0 || r.getAttribute("src") === s || (r.src = s, w(P(r)));
	}), r;
}
function L(e) {
	let t = C(), r = (() => {
		var t = k();
		return n((n) => {
			var r = e.clip.url, i = F(e.clip.fit);
			return r !== n.e && s(t, "src", n.e = r), n.t = l(t, i, n.t), n;
		}, {
			e: void 0,
			t: void 0
		}), t;
	})();
	return h(() => {
		let n = ((e.clip.trim ?? 0) + t() + .5) / e.fps;
		Math.abs(r.currentTime - n) < .5 / e.fps || (r.currentTime = n, w(Promise.race([new Promise((e) => r.addEventListener("seeked", e, { once: !0 })), new Promise((e) => setTimeout(e, 2e3))])));
	}), r;
}
function R(e) {
	let t = (() => {
		var t = O();
		return n((n) => {
			var r = e.clip.url, i = F(e.clip.fit);
			return r !== n.e && s(t, "src", n.e = r), n.t = l(t, i, n.t), n;
		}, {
			e: void 0,
			t: void 0
		}), t;
	})();
	return x() || w(P(t)), t;
}
function z(e) {
	let t = () => e.style ?? {}, i = {
		top: "flex-start",
		center: "center",
		bottom: "flex-end"
	};
	return (() => {
		var a = A();
		return r(a, () => e.text), n((e) => {
			var n = i[t().verticalAlign ?? "center"], r = t().align ?? "center", o = `${t().padding ?? 0}px`, s = t().background ?? "transparent", l = t().color ?? "#fff", u = t().fontFamily ?? "-apple-system, Helvetica, Arial, sans-serif", d = `${t().fontSize ?? 64}px`, f = String(t().fontWeight ?? 700), p = String(t().lineHeight ?? 1.2);
			return n !== e.e && c(a, "justify-content", e.e = n), r !== e.t && c(a, "text-align", e.t = r), o !== e.a && c(a, "padding", e.a = o), s !== e.o && c(a, "background", e.o = s), l !== e.i && c(a, "color", e.i = l), u !== e.n && c(a, "font-family", e.n = u), d !== e.s && c(a, "font-size", e.s = d), f !== e.h && c(a, "font-weight", e.h = f), p !== e.r && c(a, "line-height", e.r = p), e;
		}, {
			e: void 0,
			t: void 0,
			a: void 0,
			o: void 0,
			i: void 0,
			n: void 0,
			s: void 0,
			h: void 0,
			r: void 0
		}), a;
	})();
}
function B(e) {
	return (() => {
		var t = j();
		return t.firstChild, r(t, () => e.what, null), t;
	})();
}
// ../player/src/main.tsx
var V = {};
window.__karkaComponents = V;
async function H(e) {
	let t = e.tracks.flatMap((e) => e.clips.flatMap((e) => e.type === "component" && e.module ? [e.module] : []));
	await Promise.all(t.filter((e) => !(e in V)).map(async (e) => V[e] = await import(
		/* @vite-ignore */
		e
)));
}
var [U, W] = g(null);
window.__karkaSetProject = (e) => void H(e).then(() => W(e), window.__karkaFailed), (window.__karkaProject ? H(window.__karkaProject).then(() => W(window.__karkaProject)) : Promise.resolve()).then(() => {
	o(() => t(p, {
		get when() {
			return U();
		},
		children: (e) => (document.documentElement.style.width = document.body.style.width = `${e().width}px`, document.documentElement.style.height = document.body.style.height = `${e().height}px`, t(M, {
			get project() {
				return e();
			},
			components: V
		}))
	}), document.getElementById("player")), window.__karkaBooted?.();
}, window.__karkaFailed);
