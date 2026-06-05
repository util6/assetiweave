package internalplatform

import (
	"errors"
	"testing"

	"github.com/util6/assetiweave/extension/platform"
)

func TestRequiredCLIVersionConstraints(t *testing.T) {
	cases := []struct {
		name       string
		build      string
		constraint string
		want       bool
		wantErr    bool
	}{
		{"empty constraint", "1.0.0", "", true, false},
		{"dev build", "dev", ">=99.0.0", true, false},
		{"v prefix", "v1.2.3", ">=1.0.0", true, false},
		{"implicit exact", "1.2.3", "1.2.3", true, false},
		{"explicit exact mismatch", "1.2.4", "=1.2.3", false, false},
		{"greater equal", "1.2.3", ">=1.2.0", true, false},
		{"greater strict fails equal", "1.2.3", ">1.2.3", false, false},
		{"less equal", "1.2.3", "<=1.2.3", true, false},
		{"missing patch", "1.2.0", ">=1.2", true, false},
		{"malformed constraint", "dev", ">=abc", false, true},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got, err := satisfiesRequiredCLIVersion(tc.build, tc.constraint)
			if tc.wantErr {
				if err == nil {
					t.Fatal("error = nil, want parse error")
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if got != tc.want {
				t.Fatalf("satisfies = %v, want %v", got, tc.want)
			}
		})
	}
}

func TestInstallAllRequiredCLIVersionUnmetHonorsFailOpen(t *testing.T) {
	restore := SetCurrentCLIVersionForTesting("1.0.0")
	t.Cleanup(restore)
	plugin := testPlugin{
		name: "future",
		capabilities: platform.Capabilities{
			RequiredCLIVersion: ">=99.0.0",
			FailurePolicy:      platform.FailOpen,
		},
	}

	result, err := InstallAll([]platform.Plugin{plugin}, nil)

	if err != nil {
		t.Fatalf("InstallAll() error = %v", err)
	}
	if len(result.Plugins) != 0 {
		t.Fatalf("installed plugins = %d, want skipped", len(result.Plugins))
	}
}

func TestInstallAllRequiredCLIVersionUnmetFailClosedAborts(t *testing.T) {
	restore := SetCurrentCLIVersionForTesting("1.0.0")
	t.Cleanup(restore)
	plugin := testPlugin{
		name: "future",
		capabilities: platform.Capabilities{
			RequiredCLIVersion: ">=99.0.0",
			FailurePolicy:      platform.FailClosed,
		},
	}

	_, err := InstallAll([]platform.Plugin{plugin}, nil)

	var installErr *PluginInstallError
	if !errors.As(err, &installErr) || installErr.ReasonCode != ReasonCapabilityUnmet {
		t.Fatalf("error = %#v, want capability_unmet", err)
	}
}

func TestInstallAllMalformedRequiredCLIVersionFailsClosed(t *testing.T) {
	restore := SetCurrentCLIVersionForTesting("1.0.0")
	t.Cleanup(restore)
	plugin := testPlugin{
		name: "malformed",
		capabilities: platform.Capabilities{
			RequiredCLIVersion: ">=abc",
			FailurePolicy:      platform.FailOpen,
		},
	}

	_, err := InstallAll([]platform.Plugin{plugin}, nil)

	var installErr *PluginInstallError
	if !errors.As(err, &installErr) || installErr.ReasonCode != ReasonInvalidCapability {
		t.Fatalf("error = %#v, want invalid capability", err)
	}
}
