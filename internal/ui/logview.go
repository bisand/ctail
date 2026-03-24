package ui

import (
	"fmt"
	"image"
	"image/color"

	"gioui.org/font"
	"gioui.org/layout"
	"gioui.org/op/clip"
	"gioui.org/op/paint"
	"gioui.org/text"
	"gioui.org/unit"
	"gioui.org/widget"
	"gioui.org/widget/material"

	"ctail/internal/rules"
	"ctail/internal/tailer"
)

// LogView is a virtual-scrolling log display widget.
type LogView struct {
	List widget.List
}

// LinesCopy is a safe copy of a log line for rendering outside the lock.
type LinesCopy struct {
	Number int64
	Text   string
}

// NewLogView creates a LogView with vertical scrolling.
func NewLogView() *LogView {
	return &LogView{
		List: widget.List{
			List: layout.List{
				Axis:        layout.Vertical,
				ScrollToEnd: true,
			},
		},
	}
}

// SetAutoScroll enables or disables auto-scroll to bottom.
func (lv *LogView) SetAutoScroll(on bool) {
	lv.List.ScrollToEnd = on
}

// Position returns the current first visible index and visible count.
func (lv *LogView) Position() (first, count, total int) {
	p := lv.List.Position
	return p.First, p.Count, p.Length
}

// ScrollBy adjusts the first visible index by delta lines.
func (lv *LogView) ScrollBy(delta int) {
	lv.List.Position.First += delta
	if lv.List.Position.First < 0 {
		lv.List.Position.First = 0
	}
	lv.List.Position.Offset = 0
}

// ScrollToEnd scrolls to the very end of the list.
func (lv *LogView) ScrollToEnd() {
	lv.List.ScrollToEnd = true
	// Set First to a very high number so Gio jumps to end on next frame
	lv.List.Position.First = 1<<31 - 1
	lv.List.Position.Offset = 0
}

// Layout renders the log lines with highlighting.
func (lv *LogView) Layout(gtx layout.Context, th *material.Theme, colors Colors,
	lines []tailer.Line, engine *rules.Engine, showLineNumbers bool, fontSize int) layout.Dimensions {

	if len(lines) == 0 {
		return layoutEmptyState(gtx, th, colors)
	}

	sp := unit.Sp(float32(fontSize))

	style := material.List(th, &lv.List)
	style.Track.Color = colors.ScrollTrack
	style.Indicator.Color = colors.ScrollThumb

	return style.Layout(gtx, len(lines), func(gtx layout.Context, index int) layout.Dimensions {
		if index < 0 || index >= len(lines) {
			return layout.Dimensions{}
		}
		line := lines[index]
		result := engine.Apply(line.Text)
		return layoutLogLine(gtx, th, colors, tailer.Line{Number: line.Number, Text: line.Text}, result, showLineNumbers, sp)
	})
}

// LayoutFromCopy renders from a pre-copied line slice (safe outside lock).
func (lv *LogView) LayoutFromCopy(gtx layout.Context, th *material.Theme, colors Colors,
	lines []LinesCopy, engine *rules.Engine, showLineNumbers bool, fontSize int) layout.Dimensions {

	if len(lines) == 0 {
		return layoutEmptyState(gtx, th, colors)
	}

	sp := unit.Sp(float32(fontSize))

	style := material.List(th, &lv.List)
	style.Track.Color = colors.ScrollTrack
	style.Indicator.Color = colors.ScrollThumb

	return style.Layout(gtx, len(lines), func(gtx layout.Context, index int) layout.Dimensions {
		if index < 0 || index >= len(lines) {
			return layout.Dimensions{}
		}
		line := lines[index]
		result := engine.Apply(line.Text)
		return layoutLogLine(gtx, th, colors, tailer.Line{Number: line.Number, Text: line.Text}, result, showLineNumbers, sp)
	})
}

func layoutEmptyState(gtx layout.Context, th *material.Theme, colors Colors) layout.Dimensions {
	return layout.Center.Layout(gtx, func(gtx layout.Context) layout.Dimensions {
		lbl := material.Label(th, unit.Sp(16), "Open a file to start tailing (Ctrl+O)")
		lbl.Color = colors.TextMuted
		lbl.Alignment = text.Middle
		return lbl.Layout(gtx)
	})
}

func layoutLogLine(gtx layout.Context, th *material.Theme, colors Colors,
	line tailer.Line, result rules.LineResult, showLineNumbers bool, fontSize unit.Sp) layout.Dimensions {

	// Full-line background if rule matches the entire line
	var lineBg color.NRGBA
	if result.FullLine && result.Background != "" {
		lineBg = HexToNRGBA(result.Background)
	}

	return layout.Stack{}.Layout(gtx,
		layout.Expanded(func(gtx layout.Context) layout.Dimensions {
			if lineBg.A > 0 {
				defer clip.Rect{Max: gtx.Constraints.Min}.Push(gtx.Ops).Pop()
				paint.ColorOp{Color: lineBg}.Add(gtx.Ops)
				paint.PaintOp{}.Add(gtx.Ops)
			}
			return layout.Dimensions{Size: gtx.Constraints.Min}
		}),
		layout.Stacked(func(gtx layout.Context) layout.Dimensions {
			return layout.Inset{Left: unit.Dp(4), Right: unit.Dp(4), Top: unit.Dp(1), Bottom: unit.Dp(1)}.Layout(gtx,
				func(gtx layout.Context) layout.Dimensions {
					children := make([]layout.FlexChild, 0, 2)

					if showLineNumbers {
						children = append(children, layout.Rigid(func(gtx layout.Context) layout.Dimensions {
							num := fmt.Sprintf("%6d ", line.Number)
							lbl := material.Label(th, fontSize, num)
							lbl.Color = colors.TextMuted
							lbl.Font.Typeface = "Go Mono, monospace"
							return lbl.Layout(gtx)
						}))
					}

					children = append(children, layout.Flexed(1, func(gtx layout.Context) layout.Dimensions {
						return layoutHighlightedText(gtx, th, colors, line.Text, result, fontSize)
					}))

					return layout.Flex{Axis: layout.Horizontal}.Layout(gtx, children...)
				},
			)
		}),
	)
}

func layoutHighlightedText(gtx layout.Context, th *material.Theme, colors Colors,
	lineText string, result rules.LineResult, fontSize unit.Sp) layout.Dimensions {

	// Simple case: no sub-matches, render as single label
	if result.FullLine || len(result.Matches) == 0 {
		fg := colors.TextPrimary
		if result.FullLine && result.Foreground != "" {
			fg = HexToNRGBA(result.Foreground)
		}
		lbl := material.Label(th, fontSize, lineText)
		lbl.Color = fg
		lbl.Font.Typeface = "Go Mono, monospace"
		lbl.MaxLines = 1
		if result.FullLine && result.Bold {
			lbl.Font.Weight = font.Bold
		}
		if result.FullLine && result.Italic {
			lbl.Font.Style = font.Italic
		}
		return lbl.Layout(gtx)
	}

	// Complex case: multiple highlighted spans
	// Build segments from matches
	segments := buildSegments(lineText, result.Matches, colors.TextPrimary)

	children := make([]layout.FlexChild, len(segments))
	for i, seg := range segments {
		s := seg
		children[i] = layout.Rigid(func(gtx layout.Context) layout.Dimensions {
			if s.bg.A > 0 {
				// Draw background behind text
				macro := clip.Rect{Max: image.Pt(gtx.Constraints.Max.X, gtx.Constraints.Max.Y)}
				defer macro.Push(gtx.Ops).Pop()
				paint.ColorOp{Color: s.bg}.Add(gtx.Ops)
				paint.PaintOp{}.Add(gtx.Ops)
			}
			lbl := material.Label(th, fontSize, s.text)
			lbl.Color = s.fg
			lbl.Font.Typeface = "Go Mono, monospace"
			lbl.MaxLines = 1
			if s.bold {
				lbl.Font.Weight = font.Bold
			}
			if s.italic {
				lbl.Font.Style = font.Italic
			}
			return lbl.Layout(gtx)
		})
	}

	return layout.Flex{Axis: layout.Horizontal}.Layout(gtx, children...)
}

type textSegment struct {
	text   string
	fg     color.NRGBA
	bg     color.NRGBA
	bold   bool
	italic bool
}

func buildSegments(lineText string, matches []rules.Match, defaultFg color.NRGBA) []textSegment {
	if len(matches) == 0 {
		return []textSegment{{text: lineText, fg: defaultFg}}
	}

	var segments []textSegment
	pos := 0

	for _, m := range matches {
		if m.Start > pos && m.Start <= len(lineText) {
			segments = append(segments, textSegment{
				text: lineText[pos:m.Start],
				fg:   defaultFg,
			})
		}
		end := m.End
		if end > len(lineText) {
			end = len(lineText)
		}
		if m.Start < end {
			fg := defaultFg
			if m.Foreground != "" {
				fg = HexToNRGBA(m.Foreground)
			}
			var bg color.NRGBA
			if m.Background != "" {
				bg = HexToNRGBA(m.Background)
			}
			segments = append(segments, textSegment{
				text:   lineText[m.Start:end],
				fg:     fg,
				bg:     bg,
				bold:   m.Bold,
				italic: m.Italic,
			})
		}
		if end > pos {
			pos = end
		}
	}

	if pos < len(lineText) {
		segments = append(segments, textSegment{
			text: lineText[pos:],
			fg:   defaultFg,
		})
	}

	return segments
}
