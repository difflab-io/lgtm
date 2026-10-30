use crate::{
    database::Database,
    proto::lgtm::v1::{HelloReviewRequest, HelloReviewResponse, ReviewService},
    services,
};
use connectrpc::{Encodable, RequestContext, Response, ServiceRequest, ServiceResult};

pub struct ReviewApi {
    reviews: services::ReviewService,
}

impl ReviewApi {
    pub fn new(database: Database) -> Self {
        Self {
            reviews: services::ReviewService::new(database),
        }
    }
}

impl ReviewService for ReviewApi {
    #[tracing::instrument(name = "rpc_request", skip_all, fields(rpc.method = "lgtm.v1.ReviewService/HelloReview"))]
    async fn hello_review<'a>(
        &'a self,
        _context: RequestContext,
        request: ServiceRequest<'_, HelloReviewRequest>,
    ) -> ServiceResult<impl Encodable<HelloReviewResponse> + Send + use<'a>> {
        Response::ok(HelloReviewResponse::from(
            self.reviews.get_hello(request.name),
        ))
    }
}
