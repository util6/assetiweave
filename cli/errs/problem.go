package errs

type Problem struct {
	Category Category
	Subtype  Subtype
	WireType string
	Code     string
	Message  string
	Hint     string
	Details  any
	Meta     any
}

func (p *Problem) Error() string {
	if p == nil {
		return ""
	}
	return p.Message
}
