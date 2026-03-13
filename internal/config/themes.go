package config

// ThemeColors defines all CSS variable values for a theme
type ThemeColors struct {
	BgPrimary    string `json:"bg-primary"`
	BgSecondary  string `json:"bg-secondary"`
	BgSurface    string `json:"bg-surface"`
	BgHover      string `json:"bg-hover"`
	TextPrimary  string `json:"text-primary"`
	TextSecondary string `json:"text-secondary"`
	TextMuted    string `json:"text-muted"`
	Accent       string `json:"accent"`
	AccentHover  string `json:"accent-hover"`
	Border       string `json:"border"`
	Danger       string `json:"danger"`
	Success      string `json:"success"`
	Warning      string `json:"warning"`
	TabActive    string `json:"tab-active"`
	TabInactive  string `json:"tab-inactive"`
	BadgeColor   string `json:"badge-color"`
	ScrollTrack  string `json:"scrollbar-track"`
	ScrollThumb  string `json:"scrollbar-thumb"`
}

// Theme defines a complete theme with dark and light variants
type Theme struct {
	Name        string      `json:"name"`
	DisplayName string      `json:"displayName"`
	Dark        ThemeColors `json:"dark"`
	Light       ThemeColors `json:"light"`
	BuiltIn     bool        `json:"builtIn"`
}

// BuiltInThemes returns all built-in themes
func BuiltInThemes() []Theme {
	return []Theme{
		catppuccinTheme(),
		nordTheme(),
		tokyoNightTheme(),
		gruvboxTheme(),
		draculaTheme(),
		oneDarkTheme(),
		solarizedTheme(),
	}
}

func catppuccinTheme() Theme {
	return Theme{
		Name:        "catppuccin",
		DisplayName: "Catppuccin",
		BuiltIn:     true,
		Dark: ThemeColors{
			BgPrimary:    "#1e1e2e",
			BgSecondary:  "#181825",
			BgSurface:    "#313244",
			BgHover:      "#45475a",
			TextPrimary:  "#cdd6f4",
			TextSecondary: "#a6adc8",
			TextMuted:    "#6c7086",
			Accent:       "#89b4fa",
			AccentHover:  "#74c7ec",
			Border:       "#45475a",
			Danger:       "#f38ba8",
			Success:      "#a6e3a1",
			Warning:      "#f9e2af",
			TabActive:    "#1e1e2e",
			TabInactive:  "#181825",
			BadgeColor:   "#f9e2af",
			ScrollTrack:  "#181825",
			ScrollThumb:  "#45475a",
		},
		Light: ThemeColors{
			BgPrimary:    "#eff1f5",
			BgSecondary:  "#e6e9ef",
			BgSurface:    "#ccd0da",
			BgHover:      "#bcc0cc",
			TextPrimary:  "#4c4f69",
			TextSecondary: "#5c5f77",
			TextMuted:    "#9ca0b0",
			Accent:       "#1e66f5",
			AccentHover:  "#209fb5",
			Border:       "#bcc0cc",
			Danger:       "#d20f39",
			Success:      "#40a02b",
			Warning:      "#df8e1d",
			TabActive:    "#eff1f5",
			TabInactive:  "#e6e9ef",
			BadgeColor:   "#df8e1d",
			ScrollTrack:  "#e6e9ef",
			ScrollThumb:  "#bcc0cc",
		},
	}
}

func nordTheme() Theme {
	return Theme{
		Name:        "nord",
		DisplayName: "Nord",
		BuiltIn:     true,
		Dark: ThemeColors{
			BgPrimary:    "#2e3440",
			BgSecondary:  "#272c36",
			BgSurface:    "#3b4252",
			BgHover:      "#434c5e",
			TextPrimary:  "#d8dee9",
			TextSecondary: "#c0c8d8",
			TextMuted:    "#616e88",
			Accent:       "#88c0d0",
			AccentHover:  "#8fbcbb",
			Border:       "#434c5e",
			Danger:       "#bf616a",
			Success:      "#a3be8c",
			Warning:      "#ebcb8b",
			TabActive:    "#2e3440",
			TabInactive:  "#272c36",
			BadgeColor:   "#ebcb8b",
			ScrollTrack:  "#272c36",
			ScrollThumb:  "#434c5e",
		},
		Light: ThemeColors{
			BgPrimary:    "#eceff4",
			BgSecondary:  "#e5e9f0",
			BgSurface:    "#d8dee9",
			BgHover:      "#c0c8d8",
			TextPrimary:  "#2e3440",
			TextSecondary: "#3b4252",
			TextMuted:    "#7b88a1",
			Accent:       "#5e81ac",
			AccentHover:  "#81a1c1",
			Border:       "#c0c8d8",
			Danger:       "#bf616a",
			Success:      "#a3be8c",
			Warning:      "#d08770",
			TabActive:    "#eceff4",
			TabInactive:  "#e5e9f0",
			BadgeColor:   "#d08770",
			ScrollTrack:  "#e5e9f0",
			ScrollThumb:  "#c0c8d8",
		},
	}
}

func tokyoNightTheme() Theme {
	return Theme{
		Name:        "tokyo-night",
		DisplayName: "Tokyo Night",
		BuiltIn:     true,
		Dark: ThemeColors{
			BgPrimary:    "#1a1b26",
			BgSecondary:  "#16161e",
			BgSurface:    "#24283b",
			BgHover:      "#33467c",
			TextPrimary:  "#c0caf5",
			TextSecondary: "#a9b1d6",
			TextMuted:    "#565f89",
			Accent:       "#7aa2f7",
			AccentHover:  "#7dcfff",
			Border:       "#33467c",
			Danger:       "#f7768e",
			Success:      "#9ece6a",
			Warning:      "#e0af68",
			TabActive:    "#1a1b26",
			TabInactive:  "#16161e",
			BadgeColor:   "#e0af68",
			ScrollTrack:  "#16161e",
			ScrollThumb:  "#33467c",
		},
		Light: ThemeColors{
			BgPrimary:    "#d5d6db",
			BgSecondary:  "#cbccd1",
			BgSurface:    "#b4b5b9",
			BgHover:      "#9699a3",
			TextPrimary:  "#343b58",
			TextSecondary: "#4c505e",
			TextMuted:    "#8990b3",
			Accent:       "#34548a",
			AccentHover:  "#166775",
			Border:       "#9699a3",
			Danger:       "#8c4351",
			Success:      "#485e30",
			Warning:      "#8f5e15",
			TabActive:    "#d5d6db",
			TabInactive:  "#cbccd1",
			BadgeColor:   "#8f5e15",
			ScrollTrack:  "#cbccd1",
			ScrollThumb:  "#9699a3",
		},
	}
}

func gruvboxTheme() Theme {
	return Theme{
		Name:        "gruvbox",
		DisplayName: "Gruvbox",
		BuiltIn:     true,
		Dark: ThemeColors{
			BgPrimary:    "#282828",
			BgSecondary:  "#1d2021",
			BgSurface:    "#3c3836",
			BgHover:      "#504945",
			TextPrimary:  "#ebdbb2",
			TextSecondary: "#d5c4a1",
			TextMuted:    "#7c6f64",
			Accent:       "#83a598",
			AccentHover:  "#8ec07c",
			Border:       "#504945",
			Danger:       "#fb4934",
			Success:      "#b8bb26",
			Warning:      "#fabd2f",
			TabActive:    "#282828",
			TabInactive:  "#1d2021",
			BadgeColor:   "#fabd2f",
			ScrollTrack:  "#1d2021",
			ScrollThumb:  "#504945",
		},
		Light: ThemeColors{
			BgPrimary:    "#fbf1c7",
			BgSecondary:  "#f2e5bc",
			BgSurface:    "#ebdbb2",
			BgHover:      "#d5c4a1",
			TextPrimary:  "#3c3836",
			TextSecondary: "#504945",
			TextMuted:    "#928374",
			Accent:       "#427b58",
			AccentHover:  "#79740e",
			Border:       "#d5c4a1",
			Danger:       "#9d0006",
			Success:      "#79740e",
			Warning:      "#b57614",
			TabActive:    "#fbf1c7",
			TabInactive:  "#f2e5bc",
			BadgeColor:   "#b57614",
			ScrollTrack:  "#f2e5bc",
			ScrollThumb:  "#d5c4a1",
		},
	}
}

func draculaTheme() Theme {
	return Theme{
		Name:        "dracula",
		DisplayName: "Dracula",
		BuiltIn:     true,
		Dark: ThemeColors{
			BgPrimary:    "#282a36",
			BgSecondary:  "#21222c",
			BgSurface:    "#44475a",
			BgHover:      "#555972",
			TextPrimary:  "#f8f8f2",
			TextSecondary: "#d4d4d4",
			TextMuted:    "#6272a4",
			Accent:       "#bd93f9",
			AccentHover:  "#ff79c6",
			Border:       "#44475a",
			Danger:       "#ff5555",
			Success:      "#50fa7b",
			Warning:      "#f1fa8c",
			TabActive:    "#282a36",
			TabInactive:  "#21222c",
			BadgeColor:   "#f1fa8c",
			ScrollTrack:  "#21222c",
			ScrollThumb:  "#44475a",
		},
		Light: ThemeColors{
			BgPrimary:    "#f8f8f2",
			BgSecondary:  "#f0f0e8",
			BgSurface:    "#e0e0d4",
			BgHover:      "#d0d0c4",
			TextPrimary:  "#282a36",
			TextSecondary: "#44475a",
			TextMuted:    "#8791b5",
			Accent:       "#7c3aed",
			AccentHover:  "#d6409f",
			Border:       "#d0d0c4",
			Danger:       "#dc2626",
			Success:      "#16a34a",
			Warning:      "#ca8a04",
			TabActive:    "#f8f8f2",
			TabInactive:  "#f0f0e8",
			BadgeColor:   "#ca8a04",
			ScrollTrack:  "#f0f0e8",
			ScrollThumb:  "#d0d0c4",
		},
	}
}

func oneDarkTheme() Theme {
	return Theme{
		Name:        "one-dark",
		DisplayName: "One Dark",
		BuiltIn:     true,
		Dark: ThemeColors{
			BgPrimary:    "#282c34",
			BgSecondary:  "#21252b",
			BgSurface:    "#353b45",
			BgHover:      "#3e4452",
			TextPrimary:  "#abb2bf",
			TextSecondary: "#9da5b4",
			TextMuted:    "#636d83",
			Accent:       "#61afef",
			AccentHover:  "#56b6c2",
			Border:       "#3e4452",
			Danger:       "#e06c75",
			Success:      "#98c379",
			Warning:      "#e5c07b",
			TabActive:    "#282c34",
			TabInactive:  "#21252b",
			BadgeColor:   "#e5c07b",
			ScrollTrack:  "#21252b",
			ScrollThumb:  "#3e4452",
		},
		Light: ThemeColors{
			BgPrimary:    "#fafafa",
			BgSecondary:  "#f0f0f0",
			BgSurface:    "#e0e0e0",
			BgHover:      "#d0d0d0",
			TextPrimary:  "#383a42",
			TextSecondary: "#4f525e",
			TextMuted:    "#a0a1a7",
			Accent:       "#4078f2",
			AccentHover:  "#0184bc",
			Border:       "#d0d0d0",
			Danger:       "#e45649",
			Success:      "#50a14f",
			Warning:      "#c18401",
			TabActive:    "#fafafa",
			TabInactive:  "#f0f0f0",
			BadgeColor:   "#c18401",
			ScrollTrack:  "#f0f0f0",
			ScrollThumb:  "#d0d0d0",
		},
	}
}

func solarizedTheme() Theme {
	return Theme{
		Name:        "solarized",
		DisplayName: "Solarized",
		BuiltIn:     true,
		Dark: ThemeColors{
			BgPrimary:    "#002b36",
			BgSecondary:  "#073642",
			BgSurface:    "#094959",
			BgHover:      "#1a5c6e",
			TextPrimary:  "#839496",
			TextSecondary: "#93a1a1",
			TextMuted:    "#586e75",
			Accent:       "#268bd2",
			AccentHover:  "#2aa198",
			Border:       "#1a5c6e",
			Danger:       "#dc322f",
			Success:      "#859900",
			Warning:      "#b58900",
			TabActive:    "#002b36",
			TabInactive:  "#073642",
			BadgeColor:   "#b58900",
			ScrollTrack:  "#073642",
			ScrollThumb:  "#1a5c6e",
		},
		Light: ThemeColors{
			BgPrimary:    "#fdf6e3",
			BgSecondary:  "#eee8d5",
			BgSurface:    "#ddd6c1",
			BgHover:      "#ccc5af",
			TextPrimary:  "#657b83",
			TextSecondary: "#586e75",
			TextMuted:    "#93a1a1",
			Accent:       "#268bd2",
			AccentHover:  "#2aa198",
			Border:       "#ccc5af",
			Danger:       "#dc322f",
			Success:      "#859900",
			Warning:      "#b58900",
			TabActive:    "#fdf6e3",
			TabInactive:  "#eee8d5",
			BadgeColor:   "#b58900",
			ScrollTrack:  "#eee8d5",
			ScrollThumb:  "#ccc5af",
		},
	}
}
