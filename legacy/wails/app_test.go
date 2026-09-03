package main

import "testing"

func TestCompareVersions(t *testing.T) {
	tests := []struct {
		a, b string
		want int // negative, zero, or positive
	}{
		{"1.0.0", "1.0.0", 0},
		{"1.0.1", "1.0.0", 1},
		{"1.0.0", "1.0.1", -1},
		{"2.0.0", "1.9.9", 1},
		{"0.6.0", "0.5.4", 1},
		{"1.0", "1.0.0", 0},
		{"1.0.0", "1.0", 0},
		{"10.0.0", "9.0.0", 1},
		{"1.10.0", "1.9.0", 1},
		{"0.0.1", "0.0.0", 1},
		{"", "", 0},
		{"1", "2", -1},
	}
	for _, tt := range tests {
		got := compareVersions(tt.a, tt.b)
		switch {
		case tt.want == 0 && got != 0:
			t.Errorf("compareVersions(%q, %q) = %d, want 0", tt.a, tt.b, got)
		case tt.want > 0 && got <= 0:
			t.Errorf("compareVersions(%q, %q) = %d, want >0", tt.a, tt.b, got)
		case tt.want < 0 && got >= 0:
			t.Errorf("compareVersions(%q, %q) = %d, want <0", tt.a, tt.b, got)
		}
	}
}
