package suggest

import (
	"reflect"
	"testing"
)

func TestClosestRanksPrefixBeforeEditDistance(t *testing.T) {
	got := Closest("dry-rnu", []string{"engine", "dry-run", "plugin-config"}, 3)
	want := []string{"dry-run"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("Closest() = %v, want %v", got, want)
	}
}

func TestClosestIsRuneAwareAndDeterministic(t *testing.T) {
	got := Closest("源列", []string{"源列表", "资源", "配置"}, 2)
	want := []string{"源列表", "资源"}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("Closest() = %v, want %v", got, want)
	}
}

func TestClosestDropsUnrelatedCandidates(t *testing.T) {
	if got := Closest("xyz", []string{"source", "profile", "asset"}, 3); len(got) != 0 {
		t.Fatalf("Closest() = %v, want no suggestions", got)
	}
}
