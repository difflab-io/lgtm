"""Deterministic GitHub HTTP boundary; the real Octocrab client makes all requests."""
import base64
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import re
import threading
import time
from urllib.parse import parse_qs, urlsplit


class Github:
    def __init__(self):
        self.mode = 'normal'
        self.base_sha = 'base'
        self.head_sha = 'head'
        self.content_reads = 0
        self.comments_delay = 0
        self.reviews = {}
        self.review_history = {}
        self.next_review_id = 10
        self.threads = {}
        self.comments = {}
        self.writes = 0
        self.errors = []
        outer = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_args):
                pass

            def do_GET(self):
                self.handle_request()

            def do_POST(self):
                self.handle_request()

            def do_PATCH(self):
                self.handle_request()

            def do_DELETE(self):
                self.handle_request()

            def handle_request(self):
                try:
                    size = int(self.headers.get('Content-Length', 0))
                    body = json.loads(self.rfile.read(size)) if size else {}
                    status, result = outer.request(self.command, self.path, body)
                except Exception as error:
                    outer.errors.append(repr(error))
                    status, result = 500, {'message': str(error)}
                data = json.dumps(result).encode()
                self.send_response(status)
                self.send_header('Content-Type', 'application/json')
                self.send_header('Content-Length', str(len(data)))
                self.end_headers()
                self.wfile.write(data)

        self.server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.endpoint_url = f'http://127.0.0.1:{self.server.server_port}'

    def close(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join()

    def pull(self, number):
        return {'number': number, 'title': f'E2E pull {number}', 'head': {'sha': 'changed' if self.mode == 'stale' else self.head_sha}, 'base': {'sha': self.base_sha}, 'state': 'closed' if self.mode in ('closed', 'merged') else 'open', 'merged': self.mode == 'merged'}

    def request(self, method, url, body):
        parsed = urlsplit(url)
        path = parsed.path
        if path == '/__control':
            self.mode = body.get('mode', 'normal')
            self.base_sha = body.get('base', self.base_sha)
            self.head_sha = body.get('head', self.head_sha)
            self.comments_delay = body.get('comments_delay', self.comments_delay)
            if 'outdated_pr' in body:
                for thread in self.threads.get(body['outdated_pr'], []):
                    if thread.get('originalLine') is None:
                        thread['originalLine'] = thread.get('line')
                        thread['originalStartLine'] = thread.get('startLine')
                    thread['isOutdated'] = body['outdated']
                    thread['line'] = None if body['outdated'] else thread['originalLine']
                    thread['startLine'] = None if body['outdated'] else thread.get('originalStartLine')
                    thread['comments']['nodes'][0]['commit'] = {'oid': self.head_sha}
                    thread['comments']['nodes'][0]['originalCommit'] = {'oid': self.head_sha}
            if body.get('discard_pending'):
                for number, review in list(self.reviews.items()):
                    if review['state'] == 'PENDING' and body.get('discard_pr', number) == number:
                        for thread in self.threads.get(number, []):
                            thread['comments']['nodes'] = [comment for comment in thread['comments']['nodes'] if comment.get('pullRequestReview', {}).get('databaseId') != review['id']]
                        self.threads[number] = [thread for thread in self.threads.get(number, []) if thread['comments']['nodes']]
                        del self.reviews[number]
            return 200, {'writes': self.writes}
        if path == '/__status':
            return 200, {'writes': self.writes, 'reviews': list(self.reviews.values()), 'comments': self.comments, 'errors': self.errors, 'content_reads': self.content_reads, 'threads': self.threads}
        if self.mode in ('denied', 'limited'):
            return (403 if self.mode == 'denied' else 429), {'message': 'Fixture permission denied' if self.mode == 'denied' else 'Fixture rate limited'}
        if method == 'GET':
            if path == '/user':
                return 200, {'id': 1, 'login': 'e2e'}
            if path == '/search/issues':
                return 200, {'total_count': 3, 'items': [{'number': i} for i in range(1, 4)]}
            if path == '/repos/acme/repo/pulls':
                return 200, [self.pull(i) for i in range(1, 4)]
            if '/contents/' in path:
                self.content_reads += 1
                text = 'one\nold\nthree\n' if parse_qs(parsed.query).get('ref') == [self.base_sha] else 'one\nnew\nthree\n'
                return 200, {'size': len(text), 'encoding': 'base64', 'content': base64.b64encode(text.encode()).decode()}
            match = re.fullmatch(r'/repos/acme/repo/pulls/(\d+)(/files|/reviews|/comments|/reviews/\d+/comments)?', path)
            if match:
                number, suffix = int(match[1]), match[2]
                if suffix == '/files':
                    return 200, [{'filename': 'review.txt', 'status': 'modified'}]
                if suffix == '/reviews':
                    return 200, self.review_history.get(number, []) + ([self.reviews[number]] if number in self.reviews else [])
                if suffix and suffix.endswith('/comments'):
                    if suffix == '/comments' and self.reviews.get(number, {}).get('state') == 'PENDING':
                        return 200, []
                    result = []
                    for thread in self.threads.get(number, []):
                        for comment in thread['comments']['nodes']:
                            result.append({'id': comment['databaseId'], 'body': comment['body'], 'user': {'id': 1, 'login': 'e2e'}, 'updated_at': comment['updatedAt']})
                    return 200, result
                return 200, self.pull(number)
            match = re.fullmatch(r'/repos/acme/repo/issues/(\d+)/comments', path)
            if match:
                return 200, self.comments.get(int(match[1]), [])
        if method in ('PATCH', 'DELETE'):
            match = re.fullmatch(r'/repos/acme/repo/pulls/comments/(\d+)', path)
            if match:
                identifier = int(match[1])
                for number, threads in self.threads.items():
                    for thread in threads:
                        for comment in thread['comments']['nodes']:
                            if comment['databaseId'] == identifier:
                                if self.reviews[number]['state'] == 'PENDING':
                                    return 404, {'message': 'Private pending comment is not available through REST'}
                                self.writes += 1
                                if method == 'PATCH':
                                    comment['body'] = body['body']
                                    return 200, comment
                                thread['comments']['nodes'].remove(comment)
                                self.threads[number] = [t for t in threads if t['comments']['nodes']]
                                return 200, {}
                return 404, {'message': 'Comment not found'}
        if path == '/graphql':
            if self.comments_delay and 'reviewThreads' in body.get('query', ''):
                time.sleep(self.comments_delay)
            query = body['query']
            variables = body.get('variables', {})
            if 'updatePullRequestReviewComment' in query or 'deletePullRequestReviewComment' in query:
                self.writes += 1
                identifier = variables['id']
                for number, threads in self.threads.items():
                    for thread in threads:
                        for comment in thread['comments']['nodes']:
                            if comment['id'] == identifier:
                                if 'updatePullRequestReviewComment' in query:
                                    comment['body'] = variables['body']
                                    return 200, {'data': {'updatePullRequestReviewComment': {'pullRequestReviewComment': comment}}}
                                thread['comments']['nodes'].remove(comment)
                                self.threads[number] = [t for t in threads if t['comments']['nodes']]
                                return 200, {'data': {'deletePullRequestReviewComment': {'clientMutationId': None}}}
                return 200, {'errors': [{'message': 'Review comment not found'}]}
            if 'node(id:' in query:
                thread = next(t for ts in self.threads.values() for t in ts if t['id'] == variables['id'])
                return 200, {'data': {'node': {'comments': thread['comments']}}}
            if 'addPullRequestReviewThreadReply' in query:
                self.writes += 1
                data = variables['input']
                number = next(n for n, r in self.reviews.items() if r['node_id'] == data['pullRequestReviewId'])
                thread = next(t for t in self.threads[number] if t['id'] == data['pullRequestReviewThreadId'])
                comment = {'id': f'C_{self.writes + 100}', 'databaseId': self.writes + 100, 'body': data['body'], 'updatedAt': '2026-10-18T12:00:00Z', 'author': {'databaseId': 1, 'login': 'e2e'}, 'pullRequestReview': {'databaseId': self.reviews[number]['id'], 'state': 'PENDING'}}
                thread['comments']['nodes'].append(comment)
                return 200, {'data': {'addPullRequestReviewThreadReply': {'comment': comment}}}
            if 'resolveReviewThread' in query:
                self.writes += 1
                thread = next(t for ts in self.threads.values() for t in ts if t['id'] == variables['id'])
                thread['isResolved'] = 'unresolveReviewThread' not in query
                key = 'resolveReviewThread' if thread['isResolved'] else 'unresolveReviewThread'
                return 200, {'data': {key: {'thread': thread}}}
            if 'addPullRequestReviewThread' in query:
                self.writes += 1
                data = variables['input']
                number = next(n for n, r in self.reviews.items() if r['node_id'] == data['pullRequestReviewId'])
                identifier = self.writes + 100
                thread = {'id': f'T_{identifier}', 'path': data['path'], 'line': data.get('line'), 'startLine': data.get('startLine'), 'diffSide': data.get('side', 'RIGHT'), 'subjectType': data['subjectType'], 'isResolved': False, 'comments': {'nodes': [{'id': f'C_{identifier}', 'databaseId': identifier, 'body': data['body'], 'updatedAt': '2026-10-18T12:00:00Z', 'author': {'databaseId': 1, 'login': 'e2e'}, 'pullRequestReview': {'databaseId': self.reviews[number]['id'], 'state': 'PENDING'}}], 'pageInfo': {'hasNextPage': False}}}
                self.threads.setdefault(number, []).append(thread)
                if self.mode == 'draft_lost':
                    return 500, {'message': 'Response lost after draft accepted'}
                return 200, {'data': {'addPullRequestReviewThread': {'thread': thread}}}
            if 'reviewThreads' in query:
                return 200, {'data': {'repository': {'pullRequest': {'reviewThreads': {'nodes': self.threads.get(variables['number'], []), 'pageInfo': {'hasNextPage': False}}}}}}
            raise RuntimeError('Unexpected GraphQL operation: ' + query)
        if method == 'POST':
            self.writes += 1
            match = re.fullmatch(r'/repos/acme/repo/pulls/(\d+)/reviews', path)
            if match:
                number = int(match[1])
                if self.reviews.get(number, {}).get('state') == 'PENDING':
                    return 422, {'message': 'A pending review already exists'}
                if number in self.reviews:
                    self.review_history.setdefault(number, []).append(self.reviews[number])
                self.next_review_id += 1
                review = {'id': self.next_review_id, 'node_id': f'PRR_{self.next_review_id}', 'state': 'PENDING', 'body': body['body'], 'commit_id': body['commit_id'], 'user': {'id': 1, 'login': 'e2e'}}
                self.reviews[number] = review
                if self.mode == 'review_lost':
                    return 500, {'message': 'Response lost after pending review accepted'}
                return 200, review
            match = re.fullmatch(r'/repos/acme/repo/pulls/(\d+)/reviews/\d+/events', path)
            if match:
                review = self.reviews[int(match[1])]
                review['state'] = {'APPROVE': 'APPROVED', 'REQUEST_CHANGES': 'CHANGES_REQUESTED', 'COMMENT': 'COMMENTED'}[body['event']]
                review['body'] = body['body']
                for t in self.threads.get(int(match[1]), []):
                    for c in t['comments']['nodes']:
                        c['pullRequestReview']['state'] = review['state']
                if self.mode == 'lost':
                    return 500, {'message': 'Response lost after acceptance'}
                return 200, review
            match = re.fullmatch(r'/repos/acme/repo/issues/(\d+)/comments', path)
            if match:
                comment = {'id': self.writes + 200, 'body': body['body'], 'user': {'id': 1, 'login': 'e2e'}, 'updated_at': '2026-10-18T12:00:00Z'}
                self.comments.setdefault(int(match[1]), []).append(comment)
                return 200, comment
        raise RuntimeError(f'Unexpected request {method} {url}')


if __name__ == '__main__':
    import argparse
    from pathlib import Path
    import signal
    parser = argparse.ArgumentParser()
    parser.add_argument('--port-file', required=True)
    args = parser.parse_args()
    github = Github()
    Path(args.port_file).write_text(github.endpoint_url)
    done = threading.Event()
    signal.signal(signal.SIGTERM, lambda *_args: done.set())
    signal.signal(signal.SIGINT, lambda *_args: done.set())
    done.wait()
    github.close()
