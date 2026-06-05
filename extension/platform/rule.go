package platform

type Rule struct {
	Name             string     `json:"name"`
	Description      string     `json:"description,omitempty"`
	Allow            []string   `json:"allow,omitempty"`
	Deny             []string   `json:"deny,omitempty"`
	MaxRisk          Risk       `json:"max_risk,omitempty"`
	Identities       []Identity `json:"identities,omitempty"`
	AllowUnannotated bool       `json:"allow_unannotated,omitempty"`
}
