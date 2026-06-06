package platform

import "fmt"

type Risk string

const (
	RiskRead          Risk = "read"
	RiskWrite         Risk = "write"
	RiskHighRiskWrite Risk = "high-risk-write"
)

var riskRanks = map[Risk]int{
	RiskRead:          0,
	RiskWrite:         1,
	RiskHighRiskWrite: 2,
}

func ParseRisk(value string) (Risk, error) {
	if value == "" {
		return "", nil
	}
	risk := Risk(value)
	if !risk.IsValid() {
		return "", fmt.Errorf("invalid risk %q: must be read|write|high-risk-write", value)
	}
	return risk, nil
}

func (r Risk) IsValid() bool {
	_, ok := riskRanks[r]
	return ok
}

func (r Risk) Rank() (int, bool) {
	rank, ok := riskRanks[r]
	return rank, ok
}

func (r Risk) String() string {
	return string(r)
}
