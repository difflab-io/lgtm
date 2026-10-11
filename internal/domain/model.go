package domain

type Actor struct {
	Name string `json:"name"`
	Kind string `json:"kind"`
}
type Comment struct {
	ID       string `json:"id"`
	ThreadID string `json:"threadId,omitempty"`
	Resolved bool   `json:"resolved,omitempty"`
	Path     string `json:"path"`
	Line     int    `json:"line"`
	Side     string `json:"side"`
	Body     string `json:"body"`
	Actor    Actor  `json:"actor"`
	ReviewID string `json:"reviewId"`
}
type Review struct {
	ID       string `json:"id"`
	Kind     string `json:"kind"`
	Repo     string `json:"repo"`
	PR       int    `json:"pr"`
	Head     string `json:"head"`
	Status   string `json:"status"`
	Body     string `json:"body"`
	Actor    Actor  `json:"actor"`
	RemoteID int64  `json:"remoteId"`
}
type State struct {
	Reviews  []Review  `json:"reviews"`
	Comments []Comment `json:"comments"`
}
type Request struct {
	Op       string `json:"op"`
	ID       string `json:"id"`
	Kind     string `json:"kind"`
	Repo     string `json:"repo"`
	PR       int    `json:"pr"`
	Path     string `json:"path"`
	Line     int    `json:"line"`
	Side     string `json:"side"`
	Body     string `json:"body"`
	Actor    Actor  `json:"actor"`
	ReviewID string `json:"reviewId"`
	Event    string `json:"event"`
}
type Response struct {
	Repository string        `json:"repository,omitempty"`
	Pulls      []PullRequest `json:"pulls"`
	RepoPath   string        `json:"repoPath,omitempty"`
	State      State         `json:"state"`
	Files      []ChangedFile `json:"files,omitempty"`
	Text       string        `json:"text,omitempty"`
	Error      string        `json:"error,omitempty"`
}

type ChangedFile struct {
	Path   string `json:"path"`
	Status string `json:"status"`
}
type PullRequest struct {
	Number int    `json:"number"`
	Title  string `json:"title"`
	Draft  bool   `json:"draft"`
}
