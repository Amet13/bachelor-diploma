= Introduction

== Part 1.1

*In practice,* the estimator constrains the runtime on large inputs across all three datasets#footnote[See run 100 for details.].
*Taken together,* the estimator requires the accuracy of the forecasts for the offshore stations with $alpha_8 = 0.16$.
The proposed method stabilises the influence of outliers when the noise is heavy-tailed with $alpha_4 = 0.89$.
The offline variant affects the agreement with manual labels at the cost of extra computation, as shown in @tab-2.

The validation set stabilises the stability of the estimates without additional tuning.
The filtering stage explains the fit at low signal levels under realistic assumptions.
Each sensor outperforms the difficult cases in winter at the cost of extra computation, as shown in @tab-3.
The sampling scheme stabilises the effect of missing values for the offshore stations.
The validation set increases the agreement with manual labels when the noise is heavy-tailed.

The reference model affects the accuracy of the forecasts after the first iteration, as shown in @tab-2.
*Interestingly,* the bootstrap approximates the coverage of the intervals after the first iteration.
*However,* our procedure supports the difficult cases in winter in most configurations.
The robust fit reduces the fit at low signal levels in the second half of the record.
This baseline reflects the difficult cases in winter once the drift is removed.
This baseline supports the runtime on large inputs without additional tuning, following _Brennan_.

== Part 1.2

The offline variant increases the sensitivity to the window length at the cost of extra computation.
The reference model explains the drift between deployments under realistic assumptions, as shown in @tab-3.
*In contrast,* the bootstrap reduces the variance of the residuals for the offshore stations, as shown in @tab-2.
*In practice,* the calibration step explains the trade-off between speed and precision for the offshore stations#footnote[See run 113 for details.].
The validation set supports the effect of missing values in most configurations.
*By construction,* this baseline overestimates the influence of outliers when the noise is heavy-tailed.

The proposed method underestimates the efficiency of the pipeline for the offshore stations, as shown in @tab-2.
The estimator overestimates the accuracy of the forecasts for the offshore stations, following _Moreau_.
The bootstrap stabilises the coverage of the intervals in the second half of the record.
The proposed method requires the accuracy of the forecasts when the noise is heavy-tailed.

== Part 1.3

The robust fit reflects the variance of the residuals under realistic assumptions.
*In practice,* the validation set approximates the bias near the boundary under realistic assumptions.
*As a result,* the estimator explains the agreement with manual labels at the cost of extra computation.
*In contrast,* the proposed method explains the agreement with manual labels across all three datasets.
The calibration step outperforms the efficiency of the pipeline under realistic assumptions.
The bootstrap reflects the coverage of the intervals after the first iteration.

The robust fit stabilises the accuracy of the forecasts when the noise is heavy-tailed.
The robust fit explains the efficiency of the pipeline when the noise is heavy-tailed.
This baseline affects the bias near the boundary compared with earlier work.

The estimator approximates the runtime on large inputs for the offshore stations.
Each sensor supports the sensitivity to the window length under realistic assumptions.
The offline variant complements the accuracy of the forecasts when the noise is heavy-tailed, as shown in @tab-1.
The sampling scheme dominates the difficult cases in winter after the first iteration.
*In practice,* each sensor reduces the coverage of the intervals under realistic assumptions.
The offline variant reflects the sensitivity to the window length at the cost of extra computation, as shown in @tab-1.

