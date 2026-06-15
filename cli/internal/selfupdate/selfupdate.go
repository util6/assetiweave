package selfupdate

import (
	"fmt"
	"runtime"
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
	if options.GOOS == "" {
		options.GOOS = runtime.GOOS
	}
	if options.GOARCH == "" {
		options.GOARCH = runtime.GOARCH
	}
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
		result.Message = fmt.Sprintf("AssetIWeave CLI %s is up to date", report.Current)
		return result
	}
	asset, target, ok := packageAsset(report.Latest, options.GOOS, options.GOARCH)
	result.Target = target
	if !ok {
		result.Action = "manual_required"
		result.Message = fmt.Sprintf("AssetIWeave CLI %s is available; download a matching package from the release page", report.Latest)
		return result
	}
	result.Action = "update_available"
	result.PackageAsset = asset
	result.PackageURL = assetURL(report.Latest, asset)
	result.ChecksumAsset = checksumAsset(asset)
	result.ChecksumURL = assetURL(report.Latest, result.ChecksumAsset)
	result.Message = fmt.Sprintf("AssetIWeave CLI %s is available for %s", report.Latest, target)
	return result
}

func packageAsset(version, goos, goarch string) (asset, target string, ok bool) {
	switch {
	case goos == "linux" && goarch == "amd64":
		target = "linux-x64"
	case goos == "darwin" && goarch == "arm64":
		target = "macos-arm64"
	case goos == "darwin" && goarch == "amd64":
		target = "macos-x64"
	case goos == "windows" && goarch == "amd64":
		target = "windows-x64"
	default:
		if goos != "" && goarch != "" {
			target = goos + "-" + goarch
		}
		return "", target, false
	}
	extension := ".tar.gz"
	if goos == "windows" {
		extension = ".zip"
	}
	tag := releaseTag(version)
	return fmt.Sprintf("assetiweave-tools-%s-%s%s", tag, target, extension), target, true
}

func checksumAsset(asset string) string {
	return asset + ".sha256"
}

func assetURL(version, asset string) string {
	return repoURL + "/releases/download/" + releaseTag(version) + "/" + asset
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
