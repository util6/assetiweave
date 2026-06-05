package errs

import "errors"

func ProblemOf(err error) (*Problem, bool) {
	var typed TypedError
	if errors.As(err, &typed) {
		problem := typed.ProblemDetail()
		return problem, problem != nil
	}
	return nil, false
}

func CategoryOf(err error) Category {
	if problem, ok := ProblemOf(err); ok {
		return problem.Category
	}
	return CategoryInternal
}

func IsTyped(err error) bool {
	var typed TypedError
	return errors.As(err, &typed)
}
