package platform

import "fmt"

type Identity string

const (
	IdentityUser  Identity = "user"
	IdentityAgent Identity = "agent"
)

func ParseIdentity(value string) (Identity, error) {
	if value == "" {
		return "", nil
	}
	identity := Identity(value)
	if !identity.IsValid() {
		return "", fmt.Errorf("invalid identity %q: must be user|agent", value)
	}
	return identity, nil
}

func (i Identity) IsValid() bool {
	return i == IdentityUser || i == IdentityAgent
}

func (i Identity) String() string {
	return string(i)
}
