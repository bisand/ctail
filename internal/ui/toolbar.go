package ui

import (
	"image"

	"gioui.org/layout"
	"gioui.org/op/clip"
	"gioui.org/op/paint"
	"gioui.org/unit"
	"gioui.org/widget"
	"gioui.org/widget/material"
)

// Toolbar renders the toolbar with action buttons.
type Toolbar struct {
	OpenBtn   widget.Clickable
	FollowChk widget.Bool
}

// ToolbarAction indicates which toolbar action was triggered.
type ToolbarAction int

const (
	ToolbarNone ToolbarAction = iota
	ToolbarOpen
)

// Layout draws the toolbar and returns the triggered action.
func (tb *Toolbar) Layout(gtx layout.Context, th *material.Theme, colors Colors, autoScroll bool) (ToolbarAction, layout.Dimensions) {
	action := ToolbarNone

	if tb.OpenBtn.Clicked(gtx) {
		action = ToolbarOpen
	}

	// Sync checkbox state with tab's auto-scroll
	tb.FollowChk.Value = autoScroll

	dims := layout.Stack{}.Layout(gtx,
		layout.Expanded(func(gtx layout.Context) layout.Dimensions {
			size := image.Pt(gtx.Constraints.Max.X, gtx.Constraints.Min.Y)
			defer clip.Rect{Max: size}.Push(gtx.Ops).Pop()
			paint.ColorOp{Color: colors.BgSurface}.Add(gtx.Ops)
			paint.PaintOp{}.Add(gtx.Ops)
			return layout.Dimensions{Size: size}
		}),
		layout.Stacked(func(gtx layout.Context) layout.Dimensions {
			return layout.Inset{
				Top: unit.Dp(4), Bottom: unit.Dp(4),
				Left: unit.Dp(8), Right: unit.Dp(8),
			}.Layout(gtx, func(gtx layout.Context) layout.Dimensions {
				return layout.Flex{Axis: layout.Horizontal, Alignment: layout.Middle, Spacing: layout.SpaceEnd}.Layout(gtx,
					// Open file button
					layout.Rigid(func(gtx layout.Context) layout.Dimensions {
						btn := material.Button(th, &tb.OpenBtn, "📂 Open")
						btn.Background = colors.Accent
						btn.TextSize = unit.Sp(13)
						btn.Inset = layout.Inset{
							Top: unit.Dp(4), Bottom: unit.Dp(4),
							Left: unit.Dp(10), Right: unit.Dp(10),
						}
						return btn.Layout(gtx)
					}),
					// Spacer
					layout.Rigid(func(gtx layout.Context) layout.Dimensions {
						return layout.Spacer{Width: unit.Dp(16)}.Layout(gtx)
					}),
					// Follow/tail checkbox
					layout.Rigid(func(gtx layout.Context) layout.Dimensions {
						return layout.Flex{Axis: layout.Horizontal, Alignment: layout.Middle}.Layout(gtx,
							layout.Rigid(func(gtx layout.Context) layout.Dimensions {
								chk := material.CheckBox(th, &tb.FollowChk, "")
								chk.Size = unit.Dp(18)
								chk.IconColor = colors.Accent
								return chk.Layout(gtx)
							}),
							layout.Rigid(func(gtx layout.Context) layout.Dimensions {
								lbl := material.Label(th, unit.Sp(13), "Follow tail")
								lbl.Color = colors.TextSecondary
								return lbl.Layout(gtx)
							}),
						)
					}),
				)
			})
		}),
	)

	return action, dims
}
