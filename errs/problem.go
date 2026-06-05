package errs

type Problem struct {
	Category Category
	Subtype  Subtype
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
