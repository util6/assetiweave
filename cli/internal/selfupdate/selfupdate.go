package selfupdate

import (
	"fmt"
	"strings"

	"github.com/util6/assetiweave/internal/harvesters"
	"github.com/util6/assetiweave/internal/update"
)

const repoURL = "https://github.com/util6/assetiweave"

type Options struct {
	CurrentVersion string
	ManifestURL    string
	GOOS           string
	GOARCH         string
}

type Result struct {
	Checked         bool                       `json:"checked"`
	UpdateAvailable bool                       `json:"update_available"`
	Current         string                     `json:"current"`
	Latest          string                     `json:"latest,omitempty"`
	Action          string                     `json:"action"`
	Target          string                     `json:"target,omitempty"`
	ReleaseURL      string                     `json:"release_url,omitempty"`
	PackageAsset    string                     `json:"package_asset,omitempty"`
	PackageURL      string                     `json:"package_url,omitempty"`
	ChecksumAsset   string                     `json:"checksum_asset,omitempty"`
	ChecksumURL     string                     `json:"checksum_url,omitempty"`
	Installed       []string                   `json:"installed,omitempty"`
	Harvesters      []harvesters.InstallResult `json:"harvesters,omitempty"`
	Message         string                     `json:"message"`
	Error           string                     `json:"error,omitempty"`
}

func Check(options Options) Result {
	report := update.CheckAndCache(options.CurrentVersion, options.ManifestURL)
	result := Result{
		Checked:         report.Checked,
		UpdateAvailable: report.Available,
		Current:         report.Current,
		Latest:          report.Latest,
		Error:           report.Error,
	}
	if !report.Checked {
		result.Action = "check_failed"
		result.Message = "failed to check the AssetIWeave release manifest"
		return result
	}
	result.ReleaseURL = releaseURL(report.Latest)
	if !report.Available {
		result.Action = "up_to_date"
		result.Message = fmt.Sprintf("AssetIWeave CLI %s is up to date with the installed app", report.Current)
		return result
	}
	result.Action = "app_update_required"
	result.Message = fmt.Sprintf("AssetIWeave %s is available; update the desktop app because CLI tools are bundled with the app installer", report.Latest)
	return result
}

func releaseURL(version string) string {
	return repoURL + "/releases/tag/" + releaseTag(version)
}

func releaseTag(version string) string {
	version = strings.TrimSpace(version)
	version = strings.TrimPrefix(version, "v")
	version = strings.TrimPrefix(version, "V")
	return "v" + version
}
