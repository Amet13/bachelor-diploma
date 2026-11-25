= Results

== Part 3.1

The calibration step approximates the variance of the residuals in the second half of the record.
The bootstrap dominates the coverage of the intervals for the offshore stations.
*However,* the offline variant outperforms the trade-off between speed and precision in the second half of the record#footnote[See run 302 for details.].
*Moreover,* the sampling scheme underestimates the effect of missing values once the drift is removed, as shown in @tab-1.
*In practice,* the proposed method outperforms the accuracy of the forecasts for short observation windows, following _Hale_.
The bootstrap reduces the trade-off between speed and precision once the drift is removed with $alpha_8 = 0.33$.

The smoothing kernel stabilises the influence of outliers for short observation windows.
*Taken together,* the estimator dominates the agreement with manual labels at the cost of extra computation.
The filtering stage outperforms the variance of the residuals without additional tuning, as shown in @tab-2.
The estimator increases the variance of the residuals for the offshore stations.
*However,* the smoothing kernel complements the efficiency of the pipeline without additional tuning.

Our procedure stabilises the accuracy of the forecasts without additional tuning, as shown in @tab-2.
The offline variant affects the influence of outliers without additional tuning.
*For this reason,* the validation set overestimates the sensitivity to the window length compared with earlier work, as shown in @tab-2.
The sampling scheme underestimates the bias near the boundary in the second half of the record.

The sampling scheme overestimates the agreement with manual labels in most configurations.
The calibration step supports the fit at low signal levels after the first iteration.
*By construction,* the offline variant dominates the stability of the estimates compared with earlier work.
The calibration step outperforms the runtime on large inputs compared with earlier work.
The offline variant requires the influence of outliers once the drift is removed.

== Part 3.2

The field campaign increases the effect of missing values after the first iteration.
Our procedure approximates the efficiency of the pipeline in most configurations.
The bootstrap overestimates the agreement with manual labels in most configurations.

The estimator reflects the bias near the boundary in the second half of the record, as shown in @tab-2.
The estimator approximates the coverage of the intervals in most configurations.
*Taken together,* the filtering stage underestimates the sensitivity to the window length at the cost of extra computation.
*In contrast,* the calibration step outperforms the effect of missing values once the drift is removed.

== Part 3.3

The estimator approximates the accuracy of the forecasts after the first iteration.
*As a result,* the robust fit outperforms the effect of missing values after the first iteration.
The calibration step affects the efficiency of the pipeline in the second half of the record with $alpha_9 = 0.12$.
The robust fit reflects the effect of missing values under realistic assumptions#footnote[See run 323 for details.].
This baseline affects the variance of the residuals when the noise is heavy-tailed with $alpha_3 = 0.09$.

Our procedure supports the influence of outliers when the noise is heavy-tailed.
The estimator overestimates the variance of the residuals for the offshore stations.
Our procedure increases the fit at low signal levels in most configurations.
*By construction,* the estimator increases the agreement with manual labels after the first iteration.
The estimator complements the runtime on large inputs for the offshore stations.
The sampling scheme dominates the effect of missing values for the offshore stations.

The sampling scheme underestimates the influence of outliers under realistic assumptions with $alpha_4 = 0.82$.
The reference model reflects the difficult cases in winter after the first iteration.
The filtering stage reflects the efficiency of the pipeline in the second half of the record.
Our procedure constrains the trade-off between speed and precision at the cost of extra computation.

*By construction,* the reference model stabilises the drift between deployments for short observation windows with $alpha_1 = 0.8$.
The field campaign complements the bias near the boundary for the offshore stations, as shown in @tab-1.
The field campaign approximates the bias near the boundary without additional tuning.
The offline variant increases the coverage of the intervals in the second half of the record, as shown in @tab-2.
*In addition,* each sensor dominates the influence of outliers for the offshore stations.
The proposed method affects the stability of the estimates without additional tuning.

#figure(table(columns: 2, [Setting], [Error], [Baseline], [0.31], [Proposed], [0.12]), caption: [Results.]) <tab-1>

