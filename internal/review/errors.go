package review

import "fmt"

type ErrorKind int

const (
	InvalidArgument ErrorKind = iota
	NotFound
	FailedPrecondition
	UpstreamFailure
)

type DomainError struct {
	Kind    ErrorKind
	message string
	cause   error
}

func (e *DomainError) Error() string { return e.message }
func invalid(format string, args ...any) error {
	return &DomainError{Kind: InvalidArgument, message: fmt.Errorf(format, args...).Error(), cause: fmt.Errorf(format, args...)}
}
func missing(format string, args ...any) error {
	return &DomainError{Kind: NotFound, message: fmt.Errorf(format, args...).Error(), cause: fmt.Errorf(format, args...)}
}
func precondition(format string, args ...any) error {
	return &DomainError{Kind: FailedPrecondition, message: fmt.Errorf(format, args...).Error(), cause: fmt.Errorf(format, args...)}
}
func upstream(format string, args ...any) error {
	return &DomainError{Kind: UpstreamFailure, message: fmt.Errorf(format, args...).Error(), cause: fmt.Errorf(format, args...)}
}

func (e *DomainError) Unwrap() error { return e.cause }
