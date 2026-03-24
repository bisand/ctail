package ui

import (
	"image"
	"image/color"

	"gioui.org/layout"
	"gioui.org/op/clip"
	"gioui.org/op/paint"
	"gioui.org/text"
	"gioui.org/unit"
	"gioui.org/widget"
	"gioui.org/widget/material"
)

// TabBar manages the clickable state for each tab.
type TabBar struct {
	tabClicks  []widget.Clickable
	closeClicks []widget.Clickable
}

// ensure allocates enough clickables for n tabs.
func (tb *TabBar) ensure(n int) {
	for len(tb.tabClicks) < n {
		tb.tabClicks = append(tb.tabClicks, widget.Clickable{})
		tb.closeClicks = append(tb.closeClicks, widget.Clickable{})
	}
}

// Layout renders the tab bar and returns (clicked tab index, closed tab index).
// Returns -1 for no action.
func (tb *TabBar) Layout(gtx layout.Context, th *material.Theme, colors Colors, tabs []*Tab, active int) (clicked, closed int, dims layout.Dimensions) {
	clicked, closed = -1, -1
	if len(tabs) == 0 {
		dims = FillRect(gtx, colors.BgSecondary, image.Pt(gtx.Constraints.Max.X, gtx.Dp(unit.Dp(36))))
		return
	}

	tb.ensure(len(tabs))

	// Process click events
	for i := range tabs {
		if tb.tabClicks[i].Clicked(gtx) {
			clicked = i
		}
		if tb.closeClicks[i].Clicked(gtx) {
			closed = i
		}
	}

	// Render
	dims = layout.Stack{}.Layout(gtx,
		layout.Expanded(func(gtx layout.Context) layout.Dimensions {
			return FillRect(gtx, colors.BgSecondary, image.Pt(gtx.Constraints.Max.X, gtx.Constraints.Min.Y))
		}),
		layout.Stacked(func(gtx layout.Context) layout.Dimensions {
			return layout.Inset{Top: unit.Dp(2), Bottom: unit.Dp(2), Left: unit.Dp(4)}.Layout(gtx,
				func(gtx layout.Context) layout.Dimensions {
					children := make([]layout.FlexChild, len(tabs))
					for i := range tabs {
						idx := i
						children[i] = layout.Rigid(func(gtx layout.Context) layout.Dimensions {
							return layoutSingleTab(gtx, th, colors, tabs[idx], idx == active,
								&tb.tabClicks[idx], &tb.closeClicks[idx])
						})
					}
					return layout.Flex{Axis: layout.Horizontal, Spacing: layout.SpaceEnd}.Layout(gtx, children...)
				},
			)
		}),
	)
	return
}

func layoutSingleTab(gtx layout.Context, th *material.Theme, colors Colors, tab *Tab, isActive bool,
	tabClick *widget.Clickable, closeClick *widget.Clickable) layout.Dimensions {

	bg := colors.TabInactive
	fg := colors.TextSecondary
	if isActive {
		bg = colors.TabActive
		fg = colors.TextPrimary
	}

	return tabClick.Layout(gtx, func(gtx layout.Context) layout.Dimensions {
		return layout.Stack{}.Layout(gtx,
			layout.Expanded(func(gtx layout.Context) layout.Dimensions {
				rr := gtx.Dp(unit.Dp(4))
				rect := clip.RRect{
					Rect: image.Rectangle{Max: gtx.Constraints.Min},
					NW:   rr, NE: rr,
				}
				paint.FillShape(gtx.Ops, bg, rect.Op(gtx.Ops))
				return layout.Dimensions{Size: gtx.Constraints.Min}
			}),
			layout.Stacked(func(gtx layout.Context) layout.Dimensions {
				return layout.Inset{
					Top: unit.Dp(6), Bottom: unit.Dp(6),
					Left: unit.Dp(12), Right: unit.Dp(8),
				}.Layout(gtx, func(gtx layout.Context) layout.Dimensions {
					return layout.Flex{Axis: layout.Horizontal, Alignment: layout.Middle}.Layout(gtx,
						layout.Rigid(func(gtx layout.Context) layout.Dimensions {
							name := tab.Name
							if tab.HasUpdate {
								name = "● " + name
							}
							lbl := material.Label(th, unit.Sp(13), name)
							lbl.Color = fg
							lbl.MaxLines = 1
							return lbl.Layout(gtx)
						}),
						layout.Rigid(func(gtx layout.Context) layout.Dimensions {
							return layout.Inset{Left: unit.Dp(6)}.Layout(gtx,
								func(gtx layout.Context) layout.Dimensions {
									return closeClick.Layout(gtx, func(gtx layout.Context) layout.Dimensions {
										lbl := material.Label(th, unit.Sp(13), "✕")
										lbl.Color = colors.TextMuted
										lbl.Alignment = text.Middle
										return lbl.Layout(gtx)
									})
								},
							)
						}),
					)
				})
			}),
		)
	})
}

func FillRect(gtx layout.Context, col color.NRGBA, size image.Point) layout.Dimensions {
	defer clip.Rect{Max: size}.Push(gtx.Ops).Pop()
	paint.ColorOp{Color: col}.Add(gtx.Ops)
	paint.PaintOp{}.Add(gtx.Ops)
	return layout.Dimensions{Size: size}
}
